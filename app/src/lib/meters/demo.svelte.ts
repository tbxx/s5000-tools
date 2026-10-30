// A short loop of signal (a kick drum and a bass line) for the style
// previews. Runs only while a preview is on screen.

import { channel, step } from "./ballistics";

const ROLES = [
  { b: -16, sw: 2, f: 0.3, beat: 2, amt: 15, dec: 9 },
  { b: -12, sw: 3, f: 0.5, beat: 4, amt: 4, dec: 6 },
];

class Demo {
  ch = $state([channel(), channel()]);
  private users = 0;
  private raf = 0;
  private t = 0;
  private last = 0;

  /** Starts the signal; call the returned function to stop using it. */
  use(): () => void {
    if (this.users++ === 0) {
      this.last = performance.now();
      this.raf = requestAnimationFrame(this.frame);
    }
    return () => {
      if (--this.users === 0) cancelAnimationFrame(this.raf);
    };
  }

  private frame = (now: number) => {
    const dt = Math.min(0.1, (now - this.last) / 1000);
    this.last = now;
    this.t += dt;
    ROLES.forEach((r, i) => {
      const since = ((((this.t * r.beat) % 1) + 1) % 1) / r.beat;
      const db = r.b + r.sw * Math.sin(2 * Math.PI * this.t * r.f) + r.amt * Math.exp(-r.dec * since) + (Math.random() - 0.5) * 3;
      step(this.ch[i], Math.min(0, db), dt, -18);
    });
    this.raf = requestAnimationFrame(this.frame);
  };
}

export const demo = new Demo();
