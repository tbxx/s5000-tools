import type { ParamInfo } from "./api";

const NOTES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/** MIDI note name as the sampler shows it (note 60 = C3). */
export function noteName(n: number): string {
  return `${NOTES[n % 12]}${Math.floor(n / 12) - 2}`;
}

/** A parameter value as text. */
export function format(info: ParamInfo, v: number): string {
  if (info.encoding === "bool") return v ? "ON" : "OFF";
  if (info.options) return info.options[v] ?? String(v);
  if (info.units === "MIDI note") return noteName(v);
  if (info.id === "zone.pan" || info.id === "multi_part.pan") return v === 0 ? "MID" : v < 0 ? `L${-v}` : `R${v}`;
  const shown = info.display_scale ? v * info.display_scale : v;
  return String(Math.round(shown * 100) / 100);
}
