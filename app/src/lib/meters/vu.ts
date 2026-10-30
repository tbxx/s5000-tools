// The VU face's fixed geometry (a 400 x 270 drawing) and its colour themes.
// A VU needle moves with voltage, so scale positions go as 10^(VU/20).

import { NEEDLE } from "./ballistics";

const PX = 200;
const PY = 300;
/** Degrees either side of centre at -20 and +3 VU. */
const SPAN = 47;

const posOf = (vu: number) => (Math.pow(10, vu / 20) - NEEDLE.lo) / (NEEDLE.hi - NEEDLE.lo);
const angle = (pos: number) => -SPAN + pos * 2 * SPAN;
const r2 = (n: number) => Math.round(n * 100) / 100;

function pt(deg: number, r: number): [number, number] {
  const rad = (deg * Math.PI) / 180;
  return [r2(PX + r * Math.sin(rad)), r2(PY - r * Math.cos(rad))];
}

function arc(fromVu: number, toVu: number, r: number) {
  const [x1, y1] = pt(angle(posOf(fromVu)), r);
  const [x2, y2] = pt(angle(posOf(toVu)), r);
  return `M ${x1} ${y1} A ${r} ${r} 0 0 1 ${x2} ${y2}`;
}

const MAJOR = [-20, -10, -7, -5, -3, 0, 3];

export const VU_FACE = {
  angle: (pos: number) => r2(angle(pos)),
  ticks: [-20, -10, -7, -6, -5, -4, -3, -2, -1, 0, 1, 2, 3].map((v) => {
    const a = angle(posOf(v));
    const major = MAJOR.includes(v);
    const [x1, y1] = pt(a, 196);
    const [x2, y2] = pt(a, major ? 216 : 208);
    return { x1, y1, x2, y2, major, red: v >= 0 };
  }),
  labels: MAJOR.map((v) => {
    const [x, y] = pt(angle(posOf(v)), 232);
    return { x, y: y + 6, text: v > 0 ? `+${v}` : String(Math.abs(v)), red: v >= 0 };
  }),
  // The lower scale, in percent of 0 VU, as printed on real VU faces.
  pctTicks: [20, 40, 60, 80, 100].map((pc) => {
    const a = angle(posOf(20 * Math.log10(pc / 100)));
    const [x1, y1] = pt(a, 160);
    const [x2, y2] = pt(a, 168);
    const [lx, ly] = pt(a, 147);
    return { x1, y1, x2, y2, lx, ly: ly + 4, text: String(pc) };
  }),
  pctArc: arc(20 * Math.log10(0.2), 0, 168),
  mainArc: arc(-20, 0, 196),
  redArc: arc(0, 3, 196),
  redBand: arc(0, 3, 202),
  screws: (
    [
      [12, 12],
      [388, 12],
      [12, 258],
      [388, 258],
    ] as const
  ).map(([x, y], i) => {
    const r = ((i * 50 + 20) * Math.PI) / 180;
    return { x, y, x1: r2(x - 3 * Math.cos(r)), y1: r2(y - 3 * Math.sin(r)), x2: r2(x + 3 * Math.cos(r)), y2: r2(y + 3 * Math.sin(r)) };
  }),
};

interface VuTheme {
  face: [string, string, string];
  faceCy: number;
  ink: string;
  red: string;
  needle: string;
  bezel: [string, string, string];
}

export const VU_THEMES: Record<"ivory" | "amber" | "blackvu", VuTheme> = {
  ivory: {
    face: ["#fbf5e2", "#ece1c1", "#bfae80"],
    faceCy: 260,
    ink: "#1c1a16",
    red: "#c4122f",
    needle: "#141414",
    bezel: ["#4a4a4a", "#1c1c1c", "#0a0a0a"],
  },
  amber: {
    face: ["#fff4b8", "#ffc845", "#9a5408"],
    faceCy: 230,
    ink: "#2b1700",
    red: "#b0001a",
    needle: "#1a0e00",
    bezel: ["#555555", "#232323", "#0b0b0b"],
  },
  blackvu: {
    face: ["#2e2e2e", "#171717", "#050505"],
    faceCy: 240,
    ink: "#ececec",
    red: "#ff5a4f",
    needle: "#f4f4f4",
    bezel: ["#5a5a5a", "#2a2a2a", "#101010"],
  },
};
