// Linked controls: ⌘-click (Ctrl elsewhere) a control to add it to the
// linked set, again to take it out; Esc empties the set. Changing one linked
// control changes the others of the same kind: number controls (knobs,
// faders, number cells) move by the same amount, each within its own range;
// switches, and menus with the same choices, take the same value.
//
// The control being changed leads: `begin` when a drag or key press starts,
// `follow` with each new value, `end` when it is over. The others snapshot
// their value at `begin`, so a drag moves them by its whole distance.

import { SvelteSet } from "svelte/reactivity";

export interface Member {
  /** "number", "switch", or "menu:" and its choices. */
  kind: string;
  begin(): void;
  /** The leader moved by `delta` from where it began (numbers), or is now `value`. */
  follow(delta: number, value: number): void;
  end(): void;
}

class Linked {
  members = new SvelteSet<Member>();

  has(m: Member) {
    return this.members.has(m);
  }

  toggle(m: Member) {
    if (this.members.has(m)) this.members.delete(m);
    else this.members.add(m);
  }

  remove(m: Member) {
    this.members.delete(m);
  }

  clear() {
    this.members.clear();
  }

  private others(m: Member) {
    return this.members.has(m) ? [...this.members].filter((o) => o !== m && o.kind === m.kind) : [];
  }

  begin(m: Member) {
    for (const o of this.others(m)) o.begin();
  }

  follow(m: Member, delta: number, value: number) {
    for (const o of this.others(m)) o.follow(delta, value);
  }

  end(m: Member) {
    for (const o of this.others(m)) o.end();
  }

  /** A one-off change (a click, a menu, a typed value). */
  once(m: Member, delta: number, value: number) {
    this.begin(m);
    this.follow(m, delta, value);
    this.end(m);
  }
}

export const linked = new Linked();

if (typeof window !== "undefined") {
  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && linked.members.size) linked.clear();
  });
}

/** A number control's side of linking: follows the leader by the same
 *  amount from where it was when the change began. */
export function numberMember(
  get: () => number | undefined,
  range: () => [number, number],
  live: { push(v: number): void; done(v: number): void },
): Member {
  let start: number | undefined;
  let last: number | undefined;
  return {
    kind: "number",
    begin() {
      start = get();
      last = undefined;
    },
    follow(delta) {
      if (start === undefined) return;
      const [min, max] = range();
      const next = Math.min(max, Math.max(min, Math.round(start + delta)));
      if (next !== last) {
        last = next;
        live.push(next);
      }
    },
    end() {
      if (last !== undefined) live.done(last);
      start = last = undefined;
    },
  };
}
