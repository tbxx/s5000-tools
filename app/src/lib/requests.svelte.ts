// Turning a drop (or a button) into queued transfers: asks about a
// program's or multi's dependencies first, and fetches files for a drag to
// the Finder.
import { SvelteMap } from "svelte/reactivity";
import { api, kindOfFile, message, type Kind, type Line, type Place, type Source, type TransferResult } from "./api";
import { acceptDiskOps } from "./experimental";
import { queue } from "./queue.svelte";
import { app } from "./state.svelte";
import { describePlace, sourceLabel, tree } from "./tree.svelte";

export interface Transferable {
  source: Source;
  kind: Kind;
  size: number | null;
}

/** What the dependency dialog returns. */
export interface DepChoice {
  files: string[];
  replace: boolean;
  dontAsk: boolean;
}

export interface DepRequest {
  item: Transferable;
  dest: Place;
  /** This item's number and how many were dropped together. */
  index: number;
  count: number;
  lines: Line[] | null;
  error: string | null;
  resolve: (choice: DepChoice | "skip" | "cancel") => void;
}

/** Measured on hardware: gets 330 KB/s, puts 236 KB/s. Used for estimates. */
export const RATE = { get: 330 * 1024, put: 236 * 1024 };

/** Seconds a transfer of `bytes` takes, roughly, or null when the sampler copies by itself. */
export function estimate(source: Source, dest: Place, bytes: number): number | null {
  const from = sourceSampler(source);
  const to = dest.at === "mac" ? null : dest.sampler;
  if (from !== null && from === to) return null;
  const get = from !== null;
  const put = to !== null;
  return (get ? bytes / RATE.get : 0) + (put ? bytes / RATE.put : 0);
}

function sourceSampler(s: Source): number | null {
  return s.at === "mac" ? null : s.sampler;
}

function samePlace(s: Source, dest: Place): boolean {
  if (s.at === "mac") return dest.at === "mac";
  if (s.at === "memory") return dest.at === "memory" && dest.sampler === s.sampler;
  return dest.at === "disk" && dest.sampler === s.sampler && dest.disk === s.disk && dest.path.join("/") === s.path.join("/");
}

/** Whether `items` can go to `dest` (not where they already are). */
export function canDrop(items: Transferable[], dest: Place | null): boolean {
  if (!dest || !items.length) return false;
  if (dest.at !== "mac" && app.sampler(dest.sampler)?.status !== "connected") return false;
  return items.some((i) => !samePlace(i.source, dest));
}

function sourceKey(s: Source): string {
  return JSON.stringify(s);
}

class Transfers {
  /** Session-only: skip the dependency dialog and use these choices. */
  dontAsk = $state(false);
  sessionReplace = $state(false);
  dialog = $state<DepRequest | null>(null);
  /** Files already fetched for a drag to the Finder, by source and dependency setting. */
  staged = new SvelteMap<string, string[]>();
  /** Sources being fetched for the Finder. */
  staging = new SvelteMap<string, Promise<string[]>>();

  constructor() {
    // A sampler's memory or a disk folder changed: earlier fetches may be stale.
    queue.onFinished((job) => {
      const d = job.request.dest;
      if (job.status === "done") tree.reload(d);
      if (d.at !== "mac") this.invalidate(d.sampler);
    });
  }

  invalidate(sampler: number) {
    for (const k of [...this.staged.keys()]) {
      const s = JSON.parse(k.slice(k.indexOf("|") + 1)) as Source;
      if (sourceSampler(s) === sampler) this.staged.delete(k);
    }
  }

  /** Mac or Finder paths as transferables; other files are reported and left out. */
  fromPaths(paths: string[], sizes?: Map<string, number>): Transferable[] {
    const out: Transferable[] = [];
    const skipped: string[] = [];
    for (const path of paths) {
      const name = path.split(/[\\/]/).pop() ?? path;
      const kind = kindOfFile(name);
      if (kind) out.push({ source: { at: "mac", path }, kind, size: sizes?.get(path) ?? null });
      else skipped.push(name);
    }
    if (skipped.length) app.fail(`Only .wav, .akp and .akm files can be sent: left out ${skipped.join(", ")}`);
    return out;
  }

  /**
   * Queues `items` for `dest`. A program or multi with "Include dependencies"
   * on opens the dependency dialog unless "Don't ask again" was chosen.
   */
  async send(items: Transferable[], dest: Place) {
    items = items.filter((i) => !samePlace(i.source, dest));
    if (!items.length) return;
    if ((dest.at === "disk" || items.some((i) => i.source.at === "disk")) && !(await acceptDiskOps())) return;
    const deps = !!app.settings.deps;
    for (const [n, item] of items.entries()) {
      const base = {
        kind: item.kind,
        label: sourceLabel(item.source),
        from: this.from(item.source),
        to: describePlace(dest),
        size: item.size,
      };
      if (item.kind === "sample" || !deps) {
        void queue.add({ ...base, request: { source: item.source, dest, deps: { mode: "skip" }, replace: !!app.settings.replace } }).catch(() => {});
        continue;
      }
      if (this.dontAsk) {
        void queue.add({ ...base, request: { source: item.source, dest, deps: { mode: "all" }, replace: this.sessionReplace } }).catch(() => {});
        continue;
      }
      const choice = await this.ask(item, dest, n + 1, items.length);
      if (choice === "cancel") break;
      if (choice === "skip" || !choice.files.length) continue;
      if (choice.dontAsk) {
        this.dontAsk = true;
        this.sessionReplace = choice.replace;
      }
      void queue.add({ ...base, request: { source: item.source, dest, deps: { mode: "only", files: choice.files }, replace: true } }).catch(() => {});
    }
  }

  private from(s: Source): string {
    if (s.at === "mac") return "Mac";
    if (s.at === "memory") return describePlace({ at: "memory", sampler: s.sampler });
    return describePlace({ at: "disk", sampler: s.sampler, disk: s.disk, path: s.path });
  }

  private ask(item: Transferable, dest: Place, index: number, count: number) {
    return new Promise<DepChoice | "skip" | "cancel">((resolve) => {
      const req: DepRequest = { item, dest, index, count, lines: null, error: null, resolve };
      this.dialog = req;
      api
        .planTransfer(item.source, dest)
        .then((lines) => {
          if (this.dialog?.resolve === resolve) this.dialog.lines = lines;
        })
        .catch((e) => {
          if (this.dialog?.resolve === resolve) this.dialog.error = message(e);
        });
    });
  }

  answer(choice: DepChoice | "skip" | "cancel") {
    const d = this.dialog;
    this.dialog = null;
    d?.resolve(choice);
  }

  // --- Drags to the Finder --------------------------------------------------------

  private stageKey(s: Source) {
    return `${app.settings.deps ? "deps" : "item"}|${sourceKey(s)}`;
  }

  /** Paths ready for the Finder, if every item has been fetched. */
  ready(items: Transferable[]): string[] | null {
    const paths: string[] = [];
    for (const i of items) {
      if (i.source.at === "mac") paths.push(i.source.path);
      else {
        const p = this.staged.get(this.stageKey(i.source));
        if (!p) return null;
        paths.push(...p);
      }
    }
    return paths;
  }

  isStaged(s: Source) {
    return this.staged.has(this.stageKey(s));
  }

  /**
   * Fetches items into a temporary folder so they can be dragged to the
   * Finder (a native drag needs real files). Shown in the queue.
   */
  async stage(items: Transferable[]): Promise<string[]> {
    const all = await Promise.all(
      items.map((i) => {
        if (i.source.at === "mac") return Promise.resolve([i.source.path]);
        const key = this.stageKey(i.source);
        const done = this.staged.get(key);
        if (done) return Promise.resolve(done);
        let p = this.staging.get(key);
        if (!p) {
          p = this.fetchForFinder(i).then(
            (paths) => {
              this.staged.set(key, paths);
              this.staging.delete(key);
              return paths;
            },
            (e) => {
              this.staging.delete(key);
              throw e;
            },
          );
          this.staging.set(key, p);
        }
        return p;
      }),
    );
    return all.flat();
  }

  private async fetchForFinder(i: Transferable): Promise<string[]> {
    const folder = await api.dragFolder();
    const r: TransferResult = await queue.add({
      kind: i.kind,
      label: sourceLabel(i.source),
      from: this.from(i.source),
      to: "Finder",
      size: i.size,
      forFinder: true,
      request: {
        source: i.source,
        dest: { at: "mac", folder },
        deps: { mode: app.settings.deps && i.kind !== "sample" ? "all" : "skip" },
        replace: true,
      },
    });
    return r.paths;
  }
}

export const transfers = new Transfers();
