// Typed wrappers around the Tauri commands in src-tauri/src/commands.rs.
// Commands with an optional `sampler` act on the active sampler without one.
import { invoke } from "@tauri-apps/api/core";

export type Kind = "sample" | "program" | "multi";
export type Mode = "simulator" | "usb";

/** The active sampler, in the shape the other views use. */
export interface Info {
  name: string;
  os_version: string;
  wave_free: number;
  cpu_free: number;
  simulated: boolean;
}

export type Origin = { kind: "simulator" } | { kind: "usb"; key: string };

export interface SamplerMeta {
  id: number;
  /** "S5000" or "S6000". */
  product: string;
  name: string;
  /** "SIM 2", "USB 1-4": shown when several samplers are connected. */
  short_id: string;
  simulated: boolean;
  os_version: string;
  wave_free: number;
  cpu_free: number;
  status: "connected" | "lost";
  active: boolean;
}

export interface UsbDevice {
  key: string;
  label: string;
  connected: number | null;
}

export interface Sample {
  index: number;
  name: string;
  channels: number;
  frames: number;
  sample_rate: number;
  bytes: number;
}

export interface Item {
  index: number;
  name: string;
  count: number;
}

export interface Memory {
  samples: Sample[];
  programs: Item[];
  multis: Item[];
}

export interface DiskInfo {
  handle: number;
  name: string;
  writable: boolean;
  kind: string;
}

export interface DiskFile {
  name: string;
  kind: Kind | "other";
  size: number | null;
}

export interface Listing {
  folders: string[];
  files: DiskFile[];
}

export interface Entry {
  name: string;
  path: string;
  kind: "dir" | Kind;
  size: number;
}

/** Where items live. */
export type Place =
  | { at: "memory"; sampler: number }
  | { at: "mac"; folder: string }
  | { at: "disk"; sampler: number; disk: number; path: string[] };

/** The item a transfer starts from. */
export type Source =
  | { at: "memory"; sampler: number; kind: Kind; name: string }
  | { at: "mac"; path: string }
  | { at: "disk"; sampler: number; disk: number; path: string[]; file: string };

export type Deps = { mode: "skip" } | { mode: "all" } | { mode: "only"; files: string[] };

export interface Request {
  source: Source;
  dest: Place;
  deps: Deps;
  replace: boolean;
}

/** One line of a transfer plan. */
export interface Line {
  kind: Kind;
  name: string;
  file: string;
  main: boolean;
  parent: string | null;
  found: boolean;
  exists: boolean;
  size: number | null;
}

export interface TransferResult {
  done: string[];
  skipped: string[];
  missing: string[];
  paths: string[];
}

export interface ProgressEvent {
  job: number;
  item: string;
  done: number;
  total: number | null;
  step: number;
  steps: number;
}

export interface Levels {
  inputs: number[];
  outputs: number[];
}

export interface ParamInfo {
  id: string;
  scope: "program" | "keygroup" | "zone" | "multi" | "multi_part";
  label: string;
  encoding: string | null;
  min: number | null;
  max: number | null;
  units: string | null;
  display_scale: number | null;
  options: string[] | null;
  index: [string, number, number] | null;
  in_file: boolean;
  writable: boolean;
  notes: string | null;
}

export type Values = Record<string, number>;

export interface ProgramView {
  name: string;
  values: Values;
  keygroups: { values: Values; zones: { sample: string | null; values: Values }[] }[];
}

/** A multi as read from its .akm: multi values and one entry per part (from part 1). */
export interface MultiView {
  name: string;
  values: Values;
  parts: { program: string | null; values: Values }[];
}

/** Values a saved multi lacks (multi number; mute and solo per part), read live. */
export interface MultiLive {
  values: Values;
  parts: Values[];
}

export interface Settings {
  folder?: string;
  deps?: boolean;
  replace?: boolean;
  mode?: Mode;
  /** The user accepted that disk writes and loads are experimental. */
  diskOpsAccepted?: boolean;
}

type Id = number | undefined;

export const api = {
  samplers: () => invoke<SamplerMeta[]>("samplers"),
  usbDevices: () => invoke<UsbDevice[]>("usb_devices"),
  addSampler: (origin: Origin) => invoke<SamplerMeta>("add_sampler", { origin }),
  removeSampler: (id: number) => invoke<void>("remove_sampler", { id }),
  reconnectSampler: (id: number) => invoke<SamplerMeta>("reconnect_sampler", { id }),
  setActive: (id: number) => invoke<void>("set_active", { id }),
  samplerInfo: (id?: number) => invoke<SamplerMeta>("sampler_info", { id }),
  memory: (sampler?: number) => invoke<Memory>("memory", { sampler }),
  deleteItems: (sampler: Id, kind: Kind, names: string[]) => invoke<void>("delete_items", { sampler, kind, names }),
  rename: (sampler: Id, kind: Kind, name: string, newName: string) =>
    invoke<void>("rename_item", { sampler, kind, name, newName }),
  createProgram: (sampler: Id, name: string, keygroups: number) =>
    invoke<void>("create_program", { sampler, name, keygroups }),
  createMulti: (sampler: Id, name: string, parts: number) => invoke<void>("create_multi", { sampler, name, parts }),
  audition: (sampler: Id, name: string, play: boolean) => invoke<void>("audition", { sampler, name, play }),
  disks: (sampler: number) => invoke<DiskInfo[]>("disks", { sampler }),
  diskList: (sampler: number, disk: number, path: string[], sizes = true) =>
    invoke<Listing>("disk_list", { sampler, disk, path, sizes }),
  diskCreateFolder: (sampler: number, disk: number, path: string[], name: string) =>
    invoke<void>("disk_create_folder", { sampler, disk, path, name }),
  diskDelete: (sampler: number, disk: number, path: string[], files: string[], folders: string[]) =>
    invoke<void>("disk_delete", { sampler, disk, path, files, folders }),
  planTransfer: (source: Source, dest: Place) => invoke<Line[]>("plan_transfer", { source, dest }),
  transfer: (job: number, request: Request) => invoke<TransferResult>("transfer", { job, request }),
  dragFolder: () => invoke<string>("drag_folder"),
  /** macOS: starts a file-promise drag at the pointer; returns its token. */
  dragOutStart: (items: Source[], icon: number[]) => invoke<number>("drag_out_start", { items, icon }),
  /** The transfers for a promise drag finished (error = null) or failed ("cancelled" to drop quietly). */
  dragOutFinish: (token: number, error: string | null) => invoke<void>("drag_out_finish", { token, error }),
  listFolder: (path: string) => invoke<Entry[]>("list_folder", { path }),
  loadSettings: () => invoke<Settings | null>("load_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  screen: () => invoke<ArrayBuffer>("screen"),
  press: (code: number) => invoke<void>("press", { code }),
  release: (code: number) => invoke<void>("release", { code }),
  wheel: (clicks: number) => invoke<void>("wheel", { clicks }),
  meters: () => invoke<Levels>("meters"),
  paramCatalog: () => invoke<ParamInfo[]>("param_catalog"),
  readProgram: (name: string) => invoke<ProgramView>("read_program", { name }),
  readProgramFile: (path: string) => invoke<ProgramView>("read_program_file", { path }),
  readLive: (program: string, keygroup: number) => invoke<Values>("read_live", { program, keygroup }),
  writeParam: (program: string, id: string, keygroup: number | null, selector: number | null, value: number) =>
    invoke<void>("write_param", { program, id, keygroup, selector, value }),
  multiCatalog: () => invoke<ParamInfo[]>("multi_catalog"),
  readMulti: (name: string) => invoke<MultiView>("read_multi", { name }),
  readMultiFile: (path: string) => invoke<MultiView>("read_multi_file", { path }),
  readMultiLive: (multi: string) => invoke<MultiLive>("read_multi_live", { multi }),
  /** `part` counts from 0; null for multi-level settings. */
  writeMultiParam: (multi: string, id: string, part: number | null, value: number) =>
    invoke<void>("write_multi_param", { multi, id, part, value }),
  setPartProgram: (multi: string, part: number, program: string) =>
    invoke<void>("set_part_program", { multi, part, program }),
};

/** Error text from a rejected command. */
export function message(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

/** Where a file goes in a sampler's memory, by its extension. */
export function kindOfFile(name: string): Kind | null {
  const ext = name.split(".").pop()?.toLowerCase();
  return ext === "wav" ? "sample" : ext === "akp" ? "program" : ext === "akm" ? "multi" : null;
}

export const extOf: Record<Kind, string> = { sample: "wav", program: "akp", multi: "akm" };

/** Calls `tick` repeatedly, at most every `ms`, until the returned stop function is called. */
export function poll(ms: number, tick: () => Promise<void>): () => void {
  let stopped = false;
  (async () => {
    while (!stopped) {
      const start = performance.now();
      try {
        await tick();
      } catch {
        // A failed poll is retried on the next tick; the status bar shows connection loss.
      }
      const wait = ms - (performance.now() - start);
      if (wait > 0) await new Promise((r) => setTimeout(r, wait));
    }
  })();
  return () => {
    stopped = true;
  };
}
