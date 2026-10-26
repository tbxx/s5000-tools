// The transfer queue: jobs run one at a time, in order, with progress from
// `transfer-progress` events.
import { listen } from "@tauri-apps/api/event";
import { api, message, type Kind, type ProgressEvent, type TransferResult } from "./api";
import { app } from "./state.svelte";

export type JobStatus = "queued" | "running" | "done" | "error" | "cancelled";

export interface Job {
  id: number;
  direction: "get" | "put";
  label: string;
  kind: Kind;
  /** get: item name; put: file path. */
  source: string;
  folder: string;
  deps: boolean;
  replace: boolean;
  /** Expected size in bytes, when known. */
  size: number | null;
  status: JobStatus;
  item: string;
  done: number;
  total: number | null;
  note: string;
}

let nextId = 1;

class Queue {
  jobs = $state<Job[]>([]);
  running = $state(false);
  /** Bumped when a batch finishes, so the Mac folder view can reload. */
  finished = $state(0);

  async init() {
    await listen<ProgressEvent>("transfer-progress", ({ payload: p }) => {
      const job = this.jobs.find((j) => j.id === p.job);
      if (!job) return;
      job.item = p.item;
      job.done = p.done;
      job.total = p.total ?? job.total;
    });
  }

  add(job: Omit<Job, "id" | "status" | "item" | "done" | "total" | "note">) {
    this.jobs.push({ ...job, id: nextId++, status: "queued", item: "", done: 0, total: job.size, note: "" });
    void this.run();
  }

  cancel(id: number) {
    const job = this.jobs.find((j) => j.id === id);
    if (job?.status === "queued") job.status = "cancelled";
  }

  clearFinished() {
    this.jobs = this.jobs.filter((j) => j.status === "queued" || j.status === "running");
  }

  get pending() {
    return this.jobs.filter((j) => j.status === "queued" || j.status === "running").length;
  }

  private async run() {
    if (this.running) return;
    this.running = true;
    let job: Job | undefined;
    while ((job = this.jobs.find((j) => j.status === "queued"))) {
      job.status = "running";
      try {
        const r: TransferResult =
          job.direction === "get"
            ? await api.getItem(job.id, job.kind, job.source, job.folder, job.deps, job.replace)
            : await api.putFile(job.id, job.source, job.deps);
        job.status = "done";
        job.note = describe(r);
        if (job.total) job.done = job.total;
      } catch (e) {
        job.status = "error";
        job.note = message(e);
      }
    }
    this.running = false;
    this.finished++;
    await app.refresh();
  }
}

function describe(r: TransferResult): string {
  const parts = [];
  if (r.done.length > 1) parts.push(`${r.done.length} files`);
  if (r.skipped.length) parts.push(`kept existing: ${r.skipped.join(", ")}`);
  if (r.missing.length) parts.push(`not found: ${r.missing.join(", ")}`);
  return parts.join(" · ");
}

export const queue = new Queue();
