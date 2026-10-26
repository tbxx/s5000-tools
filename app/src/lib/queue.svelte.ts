// The transfer queue. Jobs on the same sampler run one at a time, in order;
// jobs on different samplers run side by side. Progress comes from
// `transfer-progress` events (at most ten a second per job).
import { listen } from "@tauri-apps/api/event";
import { api, message, type Kind, type Place, type ProgressEvent, type Request, type TransferResult } from "./api";
import { app } from "./state.svelte";

export type JobStatus = "queued" | "running" | "done" | "error" | "cancelled";

export interface JobSpec {
  request: Request;
  kind: Kind;
  /** The item, e.g. "Drums.akp". */
  label: string;
  /** Where from and to, e.g. "S5000 · SIM 1 memory", "Mac". */
  from: string;
  to: string;
  /** Expected size in bytes, when known. */
  size: number | null;
  /** A fetch for a drag to the Finder. */
  forFinder?: boolean;
}

export interface Job extends JobSpec {
  id: number;
  status: JobStatus;
  item: string;
  done: number;
  total: number | null;
  step: number;
  steps: number;
  note: string;
  started: number;
  result: TransferResult | null;
}

let nextId = 1;

function samplersOf(r: Request): number[] {
  const ids = new Set<number>();
  if (r.source.at !== "mac") ids.add(r.source.sampler);
  if (r.dest.at !== "mac") ids.add(r.dest.sampler);
  return [...ids];
}

function placeChanged(place: Place): number | null {
  return place.at === "mac" ? null : place.sampler;
}

type Listener = (job: Job) => void;

class Queue {
  jobs = $state<Job[]>([]);
  /** Bumped when a job finishes, so the Mac folder view can reload. */
  finished = $state(0);
  private waiters = new Map<number, { resolve: (r: TransferResult) => void; reject: (e: unknown) => void }>();
  private listeners: Listener[] = [];

  get running() {
    return this.jobs.some((j) => j.status === "running");
  }

  get pending() {
    return this.jobs.filter((j) => j.status === "queued" || j.status === "running").length;
  }

  async init() {
    await listen<ProgressEvent>("transfer-progress", ({ payload: p }) => {
      const job = this.jobs.find((j) => j.id === p.job);
      if (!job) return;
      job.item = p.item;
      job.done = p.done;
      job.total = p.total ?? job.total;
      job.step = p.step;
      job.steps = p.steps;
    });
  }

  /** Called when any job ends (done or failed). */
  onFinished(f: Listener): () => void {
    this.listeners.push(f);
    return () => (this.listeners = this.listeners.filter((x) => x !== f));
  }

  /** Queues a job; the promise settles when it ends. */
  add(spec: JobSpec): Promise<TransferResult> {
    const id = nextId++;
    this.jobs.push({
      ...spec,
      id,
      status: "queued",
      item: "",
      done: 0,
      total: spec.size,
      step: 0,
      steps: 0,
      note: "",
      started: 0,
      result: null,
    });
    const promise = new Promise<TransferResult>((resolve, reject) => this.waiters.set(id, { resolve, reject }));
    this.schedule();
    return promise;
  }

  cancel(id: number) {
    const job = this.jobs.find((j) => j.id === id);
    if (job?.status !== "queued") return;
    job.status = "cancelled";
    this.waiters.get(id)?.reject("cancelled");
    this.waiters.delete(id);
  }

  clearFinished() {
    this.jobs = this.jobs.filter((j) => j.status === "queued" || j.status === "running");
  }

  /** Starts every queued job whose samplers are free and not claimed by an earlier queued job. */
  private schedule() {
    const busy = new Set<number>();
    for (const j of this.jobs) if (j.status === "running") samplersOf(j.request).forEach((s) => busy.add(s));
    for (const j of this.jobs) {
      if (j.status !== "queued") continue;
      const ids = samplersOf(j.request);
      const free = ids.every((s) => !busy.has(s));
      ids.forEach((s) => busy.add(s));
      if (free) void this.start(j);
    }
  }

  private async start(job: Job) {
    job.status = "running";
    job.started = performance.now();
    const waiter = this.waiters.get(job.id);
    this.waiters.delete(job.id);
    try {
      const r = await api.transfer(job.id, $state.snapshot(job.request) as Request);
      job.result = r;
      job.status = "done";
      job.note = describe(r);
      if (job.total) job.done = job.total;
      waiter?.resolve(r);
    } catch (e) {
      job.status = "error";
      job.note = message(e);
      waiter?.reject(e);
    }
    this.finished++;
    for (const f of this.listeners) f(job);
    this.schedule();
    // What changed in memory; disks are reloaded by the tree's listener.
    const r = job.request;
    const id = r.dest.at === "memory" ? placeChanged(r.dest) : null;
    if (id !== null && !job.forFinder) await app.refresh(id);
  }
}

function describe(r: TransferResult): string {
  const parts = [];
  if (r.done.length > 1) parts.push(`${r.done.length} files`);
  if (r.done.length === 0 && r.skipped.length) parts.push("nothing to copy");
  if (r.skipped.length) parts.push(`kept existing: ${r.skipped.join(", ")}`);
  if (r.missing.length) parts.push(`not found: ${r.missing.join(", ")}`);
  return parts.join(" · ");
}

export const queue = new Queue();
