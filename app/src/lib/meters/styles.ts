// Meter styles. LED and plasma styles draw a column (or two) per channel;
// VU styles draw a moving-coil meter. Names describe a look rather than a
// brand; `note` says what each is modelled on.

export type MeterKind = "signage" | "ledbar" | "plasma" | "spot" | "twin" | "fsc" | "ivory" | "amber" | "blackvu";

export interface MeterStyle {
  id: MeterKind;
  name: string;
  family: "led" | "vu";
  note: string;
}

export const METER_STYLES: MeterStyle[] = [
  { id: "signage", name: "Signage", family: "led", note: "A flat bar: green, yellow above −12 dB, red above −3 dB." },
  {
    id: "ledbar",
    name: "Console LED",
    family: "led",
    note: "Segmented LED ladder with finer steps near the top, as on 1980s large-format consoles.",
  },
  {
    id: "plasma",
    name: "Plasma",
    family: "led",
    note: "A gas-discharge plasma bargraph: fine-pitched orange-red glow, as fitted to classic large-format consoles.",
  },
  { id: "spot", name: "Flying spot", family: "led", note: "A single dot rides the level; a second marks the peak." },
  {
    id: "twin",
    name: "Twin column",
    family: "led",
    note: "Two columns per channel: VU-ballistic level on the left, peak (PPM) on the right.",
  },
  {
    id: "fsc",
    name: "Studio console",
    family: "led",
    note: "VU and PPM in one column: a VU-ballistic bar, a peak dot and an OVL lamp.",
  },
  { id: "ivory", name: "Ivory VU", family: "vu", note: "Moving-coil VU with a cream face and a red zone above 0 VU." },
  { id: "amber", name: "Amber VU", family: "vu", note: "Backlit yellow VU with a warm glow." },
  { id: "blackvu", name: "Black VU", family: "vu", note: "Dark-face VU with a white scale." },
];

export const styleOf = (id: string | undefined): MeterStyle => METER_STYLES.find((s) => s.id === id) ?? METER_STYLES[1];

/** Shown as "no signal". */
export const SILENT = -90;
/** dBFS at the bottom of the LED scales. */
export const FLOOR = -60;

export const frac = (db: number) => Math.max(0, Math.min(1, (db - FLOOR) / -FLOOR));
const linear = (n: number) => Array.from({ length: n }, (_, i) => FLOOR + (i * -FLOOR) / (n - 1));

/** The console ladder's uneven steps, finer near the top. */
export const LADDER_STEPS = [-60, -50, -42, -36, -30, -27, -24, -21, -18, -16, -14, -12, -10, -8, -6, -5, -4, -3, -2, -1, 0];

export interface Segment {
  on: boolean;
  color: string;
  dim: string;
}

type Colors = (threshold: number) => [on: string, dim: string];

/** One column, top segment first. `dot` lights only the segment at `level`. */
export function column(steps: number[], level: number, peak: number, colorOf: Colors, dot = false): Segment[] {
  let levelIdx = -1;
  let peakIdx = -1;
  steps.forEach((t, i) => {
    if (level >= t) levelIdx = i;
    if (peak >= t) peakIdx = i;
  });
  return steps
    .map((t, i) => {
      const [color, dim] = colorOf(t);
      const lit = dot ? i === levelIdx || i === levelIdx - 1 : level >= t;
      return { on: lit || (i === peakIdx && peak > FLOOR), color, dim };
    })
    .reverse();
}

export interface LadderLook {
  columns: Segment[][];
  gap: number;
  colGap: number;
  pad: number;
  bg: string;
  radius: number;
  glow: boolean;
}

/** The segments of a ladder style for a channel's levels (all dBFS). */
export function ladder(kind: MeterKind, db: number, peak: number, vu: number): LadderLook {
  const traffic: Colors = (t) =>
    t >= -2 ? ["#ff3b30", "#3d1210"] : t >= -12 ? ["#ffd23f", "#3a3110"] : ["#3bd16f", "#0f2e1a"];
  switch (kind) {
    case "spot": {
      const spot: Colors = (t) =>
        t >= -3 ? ["#ff3b30", "#1d0c0b"] : t >= -12 ? ["#ffb020", "#1d1709"] : ["#46e07c", "#0b1a10"];
      return { columns: [column(linear(48), db, peak, spot, true)], gap: 1, colGap: 0, pad: 3, bg: "#050505", radius: 1, glow: true };
    }
    case "twin": {
      const steps = linear(40);
      const vuCol: Colors = (t) => (t >= -6 ? ["#ff5040", "#2a100c"] : ["#66e08a", "#0e2416"]);
      const ppmCol: Colors = (t) => (t >= -6 ? ["#ff5040", "#2a100c"] : ["#ffc23a", "#2a220c"]);
      return {
        columns: [column(steps, vu, SILENT, vuCol), column(steps, db, peak, ppmCol)],
        gap: 1,
        colGap: 3,
        pad: 3,
        bg: "#070707",
        radius: 0,
        glow: true,
      };
    }
    case "fsc": {
      const col: Colors = (t) =>
        t >= -4 ? ["#ff4a3a", "#22100d"] : t >= -14 ? ["#ffd84a", "#262010"] : ["#9be15d", "#141f0e"];
      return { columns: [column(linear(44), vu, db, col)], gap: 1, colGap: 0, pad: 3, bg: "#0d0f12", radius: 1, glow: true };
    }
    default:
      return { columns: [column(LADDER_STEPS, db, peak, traffic)], gap: 2, colGap: 0, pad: 3, bg: "#0b0b0b", radius: 1, glow: true };
  }
}

/** Where the dB marks sit on a style's scale, as a fraction of the bar's height. */
export function scaleMarks(kind: MeterKind): { label: string; at: number }[] {
  if (kind === "ledbar") {
    return [0, -3, -6, -10, -18, -30, -42, -60].map((v) => ({
      label: String(v),
      at: (LADDER_STEPS.indexOf(v) + 0.5) / LADDER_STEPS.length,
    }));
  }
  return [0, -6, -12, -18, -24, -30, -40, -50, -60].map((v) => ({ label: String(v), at: frac(v) }));
}
