// Windows besides the main one: the front panel, the meters, any number of
// program and multi editors, the keyboard, and Help. Each is the same page
// with a `#window=<kind>` hash (App.svelte). The main window opens and tracks
// them all; the others ask it to with an "open-window" event. Which are open
// is kept in the settings and reopened at startup.

import { emitTo, listen } from "@tauri-apps/api/event";
import { WebviewWindow, getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { app } from "./state.svelte";

export type WindowKind = "panel" | "meters" | "program" | "multi" | "keyboard" | "help";

const hash = new URLSearchParams(location.hash.slice(1));
const kindHere = hash.get("window") as WindowKind | null;

/** This window: its label, and its kind and item unless it is the main window. */
export const here = {
  label: getCurrentWebviewWindow().label,
  kind: kindHere,
  item: hash.get("item") ?? undefined,
  section: hash.get("section") ?? undefined,
};
export const isMain = here.kind === null;

const TITLES: Record<WindowKind, string> = {
  panel: "Front panel",
  meters: "Meters",
  program: "Program",
  multi: "Multi",
  keyboard: "Keyboard",
  help: "s5000-tools Help",
};

const SIZES: Record<WindowKind, [number, number]> = {
  panel: [760, 420],
  meters: [980, 520],
  program: [1100, 760],
  multi: [1180, 760],
  keyboard: [1100, 340],
  help: [820, 700],
};

export const titleFor = (kind: WindowKind, item?: string) =>
  item && kind !== "help" ? `${TITLES[kind]}: ${item}` : TITLES[kind];

interface Open {
  label: string;
  kind: WindowKind;
  item?: string;
}

class Windows {
  /** Open windows (kept by the main window). */
  open = $state<Open[]>([]);

  has(kind: WindowKind) {
    return this.open.some((w) => w.kind === kind);
  }

  /** Opens a window, or brings forward the one already showing it. From a
   *  window other than the main one, asks the main window to. */
  async show(kind: WindowKind, item?: string, section?: string) {
    if (!isMain) return void emitTo("main", "open-window", { kind, item, section });
    // One front panel, meters, keyboard and Help window; one editor per item.
    const single = kind !== "program" && kind !== "multi";
    const existing = this.open.find((w) => w.kind === kind && (single || (item !== undefined && w.item === item)));
    if (existing) {
      const w = await WebviewWindow.getByLabel(existing.label);
      if (w) {
        await w.unminimize();
        await w.setFocus();
        if (section) await emitTo(existing.label, "help-section", section);
        return;
      }
      this.forget(existing.label);
    }
    let label: string = kind;
    if (!single) for (let n = 1; this.open.some((w) => w.label === (label = `${kind}-${n}`)); n++);
    const params = new URLSearchParams({ window: kind });
    if (item) params.set("item", item);
    if (section) params.set("section", section);
    const [width, height] = SIZES[kind];
    const w = new WebviewWindow(label, {
      url: `index.html#${params}`,
      title: titleFor(kind, item),
      width,
      height,
      minWidth: 420,
      minHeight: 300,
      acceptFirstMouse: true,
    });
    w.once("tauri://error", (e) => app.fail(`Cannot open the window: ${JSON.stringify(e.payload)}`));
    w.once("tauri://destroyed", () => this.forget(label));
    this.open.push({ label, kind, item });
    this.remember();
  }

  private forget(label: string) {
    this.open = this.open.filter((w) => w.label !== label);
  }

  /** Saves which windows are open (not Help), to reopen them next time. */
  private remember() {
    app.save({ windows: this.open.filter((w) => w.kind !== "help").map(({ kind, item }) => ({ kind, item })) });
  }

  /** Main window only: serves requests from the other windows, and reopens
   *  last session's windows. */
  async init() {
    await listen<{ kind: WindowKind; item?: string; section?: string }>("open-window", ({ payload: p }) =>
      this.show(p.kind, p.item, p.section),
    );
    // A window the user closed (not one closed by quitting).
    await listen<string>("window-closed", ({ payload: label }) => {
      this.forget(label);
      this.remember();
    });
    // The item an editor window shows changed.
    await listen<{ label: string; item?: string }>("window-item", ({ payload: p }) => {
      const w = this.open.find((x) => x.label === p.label);
      if (w) {
        w.item = p.item;
        this.remember();
      }
    });
    for (const w of app.settings.windows ?? []) await this.show(w.kind, w.item);
  }

  /** Other windows: tell the main window when the user closes this one. */
  async initFloating() {
    await getCurrentWebviewWindow().onCloseRequested(() => emitTo("main", "window-closed", here.label));
  }

  /** An editor window now shows `item`: retitle it and remember it. */
  async showing(item: string | undefined) {
    if (isMain || !here.kind) return;
    await getCurrentWebviewWindow().setTitle(titleFor(here.kind, item));
    await emitTo("main", "window-item", { label: here.label, item });
  }
}

export const windows = new Windows();
