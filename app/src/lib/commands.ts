// Commands: what the menu bar's items and their shortcuts do (menu.rs has
// the menu). A shortcut can arrive twice, as a key press in the page and as
// the menu item it triggers; the second within 400 ms is ignored.
//
// The main window carries out every command. Other windows handle size and
// panes themselves and pass the rest to the main window.

import { emitTo, listen } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { cyclePane } from "./keys";
import { modKey, setZoom, stepZoom } from "./prefs.svelte";
import { here, isMain } from "./windows.svelte";

const recent = new Map<string, number>();

function fresh(id: string) {
  const now = performance.now();
  const last = recent.get(id);
  recent.set(id, now);
  return last === undefined || now - last > 400;
}

/** The command for a key press, if it is a shortcut. */
export function keyCommand(e: KeyboardEvent): string | null {
  if (e.key === "F6") return e.shiftKey ? "pane.previous" : "pane.next";
  if (!modKey(e) || e.altKey) return null;
  if (e.key === "]") return "pane.next";
  if (e.key === "[") return "pane.previous";
  if (e.key === "=" || e.key === "+") return "size.bigger";
  if (e.key === "-" || e.key === "_") return "size.smaller";
  if (e.key === "0") return "size.actual";
  if (e.key === ",") return "app.preferences";
  if (e.key === "?" || (e.shiftKey && e.code === "Slash")) return "help.guide";
  if (e.code === "KeyK" && !e.shiftKey) return "window.keyboard";
  const digit = /^Digit([1-5])$/.exec(e.code)?.[1];
  if (!digit) return null;
  const n = Number(digit) - 1;
  return e.shiftKey
    ? `window.${["main", "panel", "meters", "program", "multi"][n]}`
    : `view.${["transfers", "panel", "meters", "program", "multi"][n]}`;
}

/** Commands every window carries out itself. */
function local(id: string): boolean {
  if (id === "size.bigger") stepZoom(1);
  else if (id === "size.smaller") stepZoom(-1);
  else if (id === "size.actual") setZoom(1);
  else if (id === "pane.next") cyclePane(1);
  else if (id === "pane.previous") cyclePane(-1);
  else return false;
  return true;
}

let mainHandler: (id: string) => void = () => {};

function run(id: string) {
  if (!fresh(id) || local(id)) return;
  if (isMain) return mainHandler(id);
  // The main window does the rest; bring it forward for views and Preferences.
  if (id.startsWith("view.") || id === "app.preferences" || id === "window.main")
    void WebviewWindow.getByLabel("main").then((w) => w?.setFocus());
  void emitTo("main", "menu", { id, window: "main" });
}

/** Handles shortcuts in this window. Use as the window's keydown handler. */
export function onKey(e: KeyboardEvent) {
  const id = keyCommand(e);
  if (!id) return;
  e.preventDefault();
  run(id);
}

/** Starts listening for menu commands meant for this window. `handler` (main
 *  window only) carries out views, Preferences, windows and Help. */
export async function initCommands(handler?: (id: string) => void) {
  if (handler) mainHandler = handler;
  await listen<{ id: string; window: string }>("menu", ({ payload }) => {
    if (payload.window === here.label) run(payload.id);
  });
}
