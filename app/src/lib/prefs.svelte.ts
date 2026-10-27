// Preferences: interface zoom and connecting to samplers automatically.
// Both are kept in the settings file (state.svelte.ts).

import { getCurrentWebview } from "@tauri-apps/api/webview";
import { LogicalSize, currentMonitor, getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "./api";
import { app } from "./state.svelte";

export const isMac = /Macintosh/.test(navigator.userAgent);
/** ⌘ on macOS, Ctrl elsewhere. */
export const modKey = (e: KeyboardEvent | MouseEvent) => (isMac ? e.metaKey : e.ctrlKey);
export const modLabel = isMac ? "⌘" : "Ctrl+";

// --- Zoom -------------------------------------------------------------------------------
// The whole window scales, like browser zoom, so the layout, knobs and the
// front panel stay in proportion.

export const ZOOMS = [0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2];

export const zoom = () => app.settings.zoom ?? 1;

export async function applyZoom(z = zoom()) {
  try {
    await getCurrentWebview().setZoom(z);
  } catch (e) {
    app.fail(e);
  }
}

export function setZoom(z: number) {
  z = Math.min(ZOOMS[ZOOMS.length - 1], Math.max(ZOOMS[0], z));
  const before = zoom();
  app.save({ zoom: z });
  void applyZoom(z);
  void resizeFor(z / before);
}

/** Grows (or shrinks) the window with the zoom, within the screen, so a
 *  bigger size shows the same layout larger instead of squeezing it. */
async function resizeFor(ratio: number) {
  if (Math.abs(ratio - 1) < 0.001) return;
  try {
    const win = getCurrentWindow();
    if (await win.isMaximized()) return;
    const scale = await win.scaleFactor();
    const size = (await win.innerSize()).toLogical(scale);
    const monitor = await currentMonitor();
    const max = monitor ? monitor.workArea.size.toLogical(monitor.scaleFactor) : null;
    const w = Math.round(Math.min(size.width * ratio, max ? max.width - 40 : Infinity));
    const h = Math.round(Math.min(size.height * ratio, max ? max.height - 60 : Infinity));
    await win.setSize(new LogicalSize(w, h));
  } catch {
    // Resizing is a convenience; the zoom itself has applied.
  }
}

/** One step bigger (1) or smaller (-1). */
export function stepZoom(dir: 1 | -1) {
  const z = zoom();
  const next = dir > 0 ? ZOOMS.find((v) => v > z + 0.001) : [...ZOOMS].reverse().find((v) => v < z - 0.001);
  if (next !== undefined) setZoom(next);
}

// --- Connecting automatically -----------------------------------------------------------
// "startup": every S5000/S6000 found when the app starts. "always": also any
// switched on or plugged in later. USB ids change on every power cycle or
// replug, so this connects whatever is found rather than remembering ids.

export type AutoConnect = "off" | "startup" | "always";

export const autoConnect = (): AutoConnect => app.settings.autoConnect ?? "off";

/** USB keys that failed to open (e.g. in use by ak.Sys); retried once they reappear. */
const failed = new Set<string>();

/** Connects every USB sampler found that isn't connected, skipped or failing. */
export async function connectAll() {
  let devices;
  try {
    devices = await api.usbDevices();
  } catch {
    return;
  }
  const present = new Set(devices.map((d) => d.key));
  // A sampler that went away (unplugged, switched off) gets a fresh chance.
  for (const k of [...failed]) if (!present.has(k)) failed.delete(k);
  for (const k of [...app.dismissedUsb]) if (!present.has(k)) app.dismissedUsb.delete(k);
  for (const d of devices) {
    if (d.connected !== null || failed.has(d.key) || app.dismissedUsb.has(d.key) || app.connecting) continue;
    const s = await app.add({ kind: "usb", key: d.key });
    if (!s) failed.add(d.key);
    else if (!app.active) await app.setActive(s.id);
  }
}
