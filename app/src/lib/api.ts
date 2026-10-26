// Typed wrappers around the Tauri commands in src-tauri/src/commands.rs.
import { invoke } from "@tauri-apps/api/core";

export type Kind = "sample" | "program" | "multi";
export type Mode = "simulator" | "usb";

export interface Info {
  name: string;
  os_version: string;
  wave_free: number;
  cpu_free: number;
  simulated: boolean;
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

export interface Entry {
  name: string;
  path: string;
  kind: "dir" | Kind;
  size: number;
}

export interface TransferResult {
  done: string[];
  skipped: string[];
  missing: string[];
}

export interface ProgressEvent {
  job: number;
  item: string;
  done: number;
  total: number | null;
}

export interface Levels {
  inputs: number[];
  outputs: number[];
}

export interface ParamInfo {
  id: string;
  scope: "program" | "keygroup" | "zone";
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

export interface Settings {
  folder?: string;
  deps?: boolean;
  replace?: boolean;
  mode?: Mode;
}

export const api = {
  connect: (mode: Mode) => invoke<Info>("connect", { mode }),
  disconnect: () => invoke<void>("disconnect"),
  info: () => invoke<Info>("info"),
  memory: () => invoke<Memory>("memory"),
  deleteItems: (kind: Kind, names: string[]) => invoke<void>("delete_items", { kind, names }),
  rename: (kind: Kind, name: string, newName: string) =>
    invoke<void>("rename_item", { kind, name, newName }),
  createProgram: (name: string, keygroups: number) => invoke<void>("create_program", { name, keygroups }),
  createMulti: (name: string, parts: number) => invoke<void>("create_multi", { name, parts }),
  audition: (name: string, play: boolean) => invoke<void>("audition", { name, play }),
  getItem: (job: number, kind: Kind, name: string, folder: string, deps: boolean, replace: boolean) =>
    invoke<TransferResult>("get_item", { job, kind, name, folder, deps, replace }),
  putFile: (job: number, path: string, deps: boolean) => invoke<TransferResult>("put_file", { job, path, deps }),
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
