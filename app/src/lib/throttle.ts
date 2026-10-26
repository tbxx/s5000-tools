// The live-write path for continuous edits (knobs, envelope graphs): at most
// one write every `ms` (20 a second by default), the latest value wins, and
// `done` sends the final value at once if it was not sent yet.

export interface Throttled {
  /** A new value while dragging. */
  push(v: number): void;
  /** The drag ended (or a one-off change): send `v` now unless it was just sent. */
  done(v: number): void;
}

export function throttled(send: (v: number) => void, ms = 50): Throttled {
  let lastAt = 0;
  let lastSent: number | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;

  function now(v: number) {
    if (timer) clearTimeout(timer);
    timer = null;
    lastAt = performance.now();
    lastSent = v;
    send(v);
  }

  return {
    push(v) {
      if (timer) clearTimeout(timer);
      timer = null;
      const wait = ms - (performance.now() - lastAt);
      if (wait <= 0) now(v);
      else timer = setTimeout(() => now(v), wait);
    },
    done(v) {
      if (timer || lastSent !== v) now(v);
    },
  };
}
