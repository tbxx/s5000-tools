// The memory tree: one root per sampler with Multis, Programs, Samples and
// Disks; disks open into folders and files, loaded when first expanded.
import { SvelteMap, SvelteSet } from "svelte/reactivity";
import { api, extOf, message, type DiskInfo, type Kind, type Listing, type Place, type Source } from "./api";
import { app, type SamplerView } from "./state.svelte";

export type RowType = "root" | "group" | "item" | "disk" | "folder" | "file" | "note";
export type Group = "multis" | "programs" | "samples" | "disks";

export interface Row {
  key: string;
  type: RowType;
  depth: number;
  sampler: number;
  label: string;
  detail: string;
  size: number | null;
  expandable: boolean;
  expanded: boolean;
  /** Where something dropped on this row goes. */
  drop: Place | null;
  /** What dragging this row transfers. */
  source: Source | null;
  kind: Kind | "other" | null;
  group?: Group;
  disk?: number;
  /** The folder a disk, folder or file row is in (for a folder: itself). */
  path?: string[];
  /** Item name (memory), file name (disk) or folder name. */
  name?: string;
  note?: "loading" | "empty" | "error";
}

interface Loaded<T> {
  value: T | null;
  loading: boolean;
  error: string | null;
}

export const folderKey = (sampler: number, disk: number, path: string[]) => `${sampler}|d|${disk}|${path.join("/")}`;
export const itemKey = (sampler: number, kind: Kind, name: string) => `${sampler}|m|${kind}|${name}`;
export const fileKey = (sampler: number, disk: number, path: string[], name: string) =>
  `${sampler}|f|${disk}|${path.join("/")}|${name}`;

const groups: { id: Group; title: string; kind?: Kind }[] = [
  { id: "multis", title: "Multis", kind: "multi" },
  { id: "programs", title: "Programs", kind: "program" },
  { id: "samples", title: "Samples", kind: "sample" },
  { id: "disks", title: "Disks" },
];

function sampleDetail(rate: number, channels: number, frames: number) {
  const ch = channels === 2 ? "stereo" : channels === 1 ? "mono" : `${channels} ch`;
  return `${(rate / 1000).toFixed(1)} kHz ${ch} · ${(frames / (rate || 1)).toFixed(2)} s`;
}

class Tree {
  /** Rows whose open state differs from their default. */
  private toggled = new SvelteSet<string>();
  disks = new SvelteMap<number, Loaded<DiskInfo[]>>();
  folders = new SvelteMap<string, Loaded<Listing>>();

  private defaultOpen(type: RowType, group?: Group) {
    return type === "root" || (type === "group" && group !== "disks");
  }

  isOpen(key: string, type: RowType, group?: Group) {
    return this.defaultOpen(type, group) !== this.toggled.has(key);
  }

  setOpen(row: Row, open: boolean) {
    if (row.expanded === open || !row.expandable) return;
    if (this.toggled.has(row.key)) this.toggled.delete(row.key);
    else this.toggled.add(row.key);
    if (!open) return;
    if (row.type === "group" && row.group === "disks" && !this.disks.has(row.sampler)) void this.loadDisks(row.sampler);
    if ((row.type === "disk" || row.type === "folder") && row.disk !== undefined && row.path) {
      if (!this.folders.has(folderKey(row.sampler, row.disk, row.path))) void this.loadFolder(row.sampler, row.disk, row.path);
    }
  }

  toggle(row: Row) {
    this.setOpen(row, !row.expanded);
  }

  async loadDisks(sampler: number, rescan = false) {
    this.disks.set(sampler, { value: this.disks.get(sampler)?.value ?? null, loading: true, error: null });
    try {
      this.disks.set(sampler, { value: await api.disks(sampler, rescan), loading: false, error: null });
    } catch (e) {
      this.disks.set(sampler, { value: null, loading: false, error: message(e) });
    }
  }

  async loadFolder(sampler: number, disk: number, path: string[]) {
    const key = folderKey(sampler, disk, path);
    this.folders.set(key, { value: this.folders.get(key)?.value ?? null, loading: true, error: null });
    try {
      this.folders.set(key, { value: await api.diskList(sampler, disk, path), loading: false, error: null });
    } catch (e) {
      this.folders.set(key, { value: null, loading: false, error: message(e) });
    }
  }

  /** Reloads a disk folder after something was written to it, if it was loaded. */
  reload(place: Place) {
    if (place.at !== "disk") return;
    if (this.folders.has(folderKey(place.sampler, place.disk, place.path))) void this.loadFolder(place.sampler, place.disk, place.path);
  }

  /** Forgets a removed sampler. */
  forget(sampler: number) {
    this.disks.delete(sampler);
    for (const k of [...this.folders.keys()]) if (k.startsWith(`${sampler}|`)) this.folders.delete(k);
  }

  /** The visible rows, in order. */
  rows(samplers: SamplerView[]): Row[] {
    const out: Row[] = [];
    for (const s of samplers) this.sampler(s, out);
    return out;
  }

  private sampler(s: SamplerView, out: Row[]) {
    const memory: Place = { at: "memory", sampler: s.id };
    const base = { sampler: s.id, size: null, source: null, kind: null, detail: "" };
    const rootKey = `${s.id}|root`;
    const rootOpen = this.isOpen(rootKey, "root");
    const lost = s.status !== "connected";
    out.push({
      ...base,
      key: rootKey,
      type: "root",
      depth: 0,
      label: app.label(s.id),
      detail: lost ? "connection lost" : `OS ${s.os_version} · wave ${s.wave_free}% free`,
      expandable: true,
      expanded: rootOpen,
      drop: lost ? null : memory,
    });
    if (!rootOpen || lost) return;
    const counts: Record<Group, number | null> = {
      multis: s.memory.multis.length,
      programs: s.memory.programs.length,
      samples: s.memory.samples.length,
      disks: this.disks.get(s.id)?.value?.length ?? null,
    };
    for (const g of groups) {
      const key = `${s.id}|g|${g.id}`;
      const open = this.isOpen(key, "group", g.id);
      out.push({
        ...base,
        key,
        type: "group",
        depth: 1,
        group: g.id,
        label: g.title,
        detail: counts[g.id] === null ? "" : String(counts[g.id]),
        expandable: true,
        expanded: open,
        drop: memory,
      });
      if (!open) continue;
      if (g.kind) this.memoryItems(s, g.kind, out);
      else this.diskRows(s, out);
    }
  }

  private memoryItems(s: SamplerView, kind: Kind, out: Row[]) {
    const memory: Place = { at: "memory", sampler: s.id };
    const items =
      kind === "sample"
        ? s.memory.samples.map((x) => ({
            name: x.name,
            detail: sampleDetail(x.sample_rate, x.channels, x.frames),
            size: x.bytes + 44,
          }))
        : kind === "program"
          ? s.memory.programs.map((x) => ({ name: x.name, detail: `${x.count} keygroup${x.count === 1 ? "" : "s"}`, size: null }))
          : s.memory.multis.map((x) => ({ name: x.name, detail: `${x.count} parts`, size: null }));
    if (!items.length) out.push(this.note(s.id, `${s.id}|none|${kind}`, 2, s.loading ? "loading" : "empty", memory));
    for (const it of items) {
      out.push({
        key: itemKey(s.id, kind, it.name),
        type: "item",
        depth: 2,
        sampler: s.id,
        label: it.name,
        name: it.name,
        detail: it.detail,
        size: it.size,
        kind,
        expandable: false,
        expanded: false,
        drop: memory,
        source: { at: "memory", sampler: s.id, kind, name: it.name },
      });
    }
  }

  private note(sampler: number, key: string, depth: number, note: Row["note"], drop: Place | null, text?: string): Row {
    return {
      key,
      type: "note",
      depth,
      sampler,
      label: text ?? (note === "loading" ? "Loading…" : "none"),
      detail: "",
      size: null,
      source: null,
      kind: null,
      expandable: false,
      expanded: false,
      drop,
      note,
    };
  }

  private diskRows(s: SamplerView, out: Row[]) {
    const memory: Place = { at: "memory", sampler: s.id };
    const d = this.disks.get(s.id);
    if (!d || d.loading) return void out.push(this.note(s.id, `${s.id}|disks|note`, 2, "loading", memory));
    if (d.error) return void out.push(this.note(s.id, `${s.id}|disks|note`, 2, "error", memory, d.error));
    if (!d.value?.length) return void out.push(this.note(s.id, `${s.id}|disks|note`, 2, "empty", memory, "no disks"));
    for (const disk of d.value) {
      const key = folderKey(s.id, disk.handle, []);
      const open = this.isOpen(key, "disk");
      out.push({
        key,
        type: "disk",
        depth: 2,
        sampler: s.id,
        label: disk.name || `Disk ${disk.handle}`,
        detail: `${disk.kind}${disk.writable ? "" : " · read-only"}`,
        size: null,
        source: null,
        kind: null,
        disk: disk.handle,
        path: [],
        expandable: true,
        expanded: open,
        drop: { at: "disk", sampler: s.id, disk: disk.handle, path: [] },
      });
      if (open) this.folderRows(s.id, disk.handle, [], 3, out);
    }
  }

  private folderRows(sampler: number, disk: number, path: string[], depth: number, out: Row[]) {
    const here: Place = { at: "disk", sampler, disk, path };
    const f = this.folders.get(folderKey(sampler, disk, path));
    const noteKey = `${folderKey(sampler, disk, path)}|note`;
    if (!f || (f.loading && !f.value)) return void out.push(this.note(sampler, noteKey, depth, "loading", here));
    if (f.error) return void out.push(this.note(sampler, noteKey, depth, "error", here, f.error));
    const listing = f.value!;
    if (!listing.folders.length && !listing.files.length) out.push(this.note(sampler, noteKey, depth, "empty", here, "empty"));
    for (const name of listing.folders) {
      const sub = [...path, name];
      const key = folderKey(sampler, disk, sub);
      const open = this.isOpen(key, "folder");
      out.push({
        key,
        type: "folder",
        depth,
        sampler,
        label: name,
        name,
        detail: "",
        size: null,
        source: null,
        kind: null,
        disk,
        path: sub,
        expandable: true,
        expanded: open,
        drop: { at: "disk", sampler, disk, path: sub },
      });
      if (open) this.folderRows(sampler, disk, sub, depth + 1, out);
    }
    for (const file of listing.files) {
      out.push({
        key: fileKey(sampler, disk, path, file.name),
        type: "file",
        depth,
        sampler,
        label: file.name,
        name: file.name,
        detail: "",
        size: file.size,
        kind: file.kind,
        disk,
        path,
        expandable: false,
        expanded: false,
        drop: here,
        source: file.kind === "other" ? null : { at: "disk", sampler, disk, path, file: file.name },
      });
    }
  }
}

export const tree = new Tree();

/** A short description of a place, e.g. "S5000 · SIM 2 memory", "SIM DISK/DRUMS", "Mac". */
export function describePlace(p: Place): string {
  if (p.at === "mac") return "Mac";
  if (p.at === "memory") return `${app.label(p.sampler)} memory`;
  const disk = tree.disks.get(p.sampler)?.value?.find((d) => d.handle === p.disk)?.name ?? `disk ${p.disk}`;
  const where = [disk, ...p.path].join("/");
  return app.samplers.length > 1 ? `${app.label(p.sampler)} ${where}` : where;
}

/** The file name a source has at its destination. */
export function sourceLabel(s: Source): string {
  if (s.at === "memory") return `${s.name}.${extOf[s.kind]}`;
  if (s.at === "mac") return s.path.split(/[\\/]/).pop() ?? s.path;
  return s.file;
}
