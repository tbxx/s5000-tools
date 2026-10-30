// How each channel's display moves between the sampler's readings (about
// every 60 ms): run once per animation frame with the latest reading.

import { SILENT } from "./styles";

export interface Channel {
  /** LED level: follows peaks up at once, falls at FALL dB/s. */
  disp: number;
  /** Peak hold. */
  peak: number;
  age: number;
  /** VU-ballistic level in dBFS (300 ms average). */
  vu: number;
  /** VU needle: 0 at -20 VU, 1 at +3 VU, and its velocity. */
  pos: number;
  vel: number;
}

const FALL = 26;
const HOLD = 1.5;
const PEAK_FALL = 20;
const VU_TIME = 0.3;
// The needle is a damped spring: about 300 ms to 99% with ~1.3% overshoot.
const WN = 19;
const ZETA = 0.81;
const VU_LO = Math.pow(10, -20 / 20);
const VU_HI = Math.pow(10, 3 / 20);
/** The sampler reports peaks; a VU reads roughly this much lower on music. */
const CREST = 3;

export const channel = (): Channel => ({ disp: SILENT, peak: SILENT, age: 0, vu: SILENT, pos: -0.04, vel: 0 });

/** Advances `c` by `dt` seconds toward the reading `inst` (dBFS, SILENT for none). */
export function step(c: Channel, inst: number, dt: number, vuRef: number) {
  c.disp = Math.max(inst, c.disp - FALL * dt);
  c.age += dt;
  if (inst >= c.peak) {
    c.peak = inst;
    c.age = 0;
  } else if (c.age > HOLD) {
    c.peak = Math.max(c.disp, c.peak - PEAK_FALL * dt);
  }
  const rms = inst <= SILENT ? SILENT : inst - CREST;
  c.vu = c.vu <= SILENT || rms <= SILENT ? rms : c.vu + (rms - c.vu) * (1 - Math.exp(-dt / VU_TIME));
  // The needle follows voltage, not decibels.
  const amp = rms <= SILENT ? 0 : Math.pow(10, (rms - vuRef) / 20);
  const target = Math.min(1.12, (amp - VU_LO) / (VU_HI - VU_LO));
  const steps = Math.max(1, Math.ceil(dt / 0.004));
  const h = dt / steps;
  for (let s = 0; s < steps; s++) {
    c.vel += (WN * WN * (target - c.pos) - 2 * ZETA * WN * c.vel) * h;
    c.pos += c.vel * h;
  }
  c.pos = Math.max(-0.05, Math.min(1.1, c.pos));
}

/** The VU reading shown under a needle, from its position. */
export function needleVu(pos: number): number {
  const a = pos * (VU_HI - VU_LO) + VU_LO;
  return a > 0 ? 20 * Math.log10(a) : -40;
}

export const NEEDLE = { lo: VU_LO, hi: VU_HI };
