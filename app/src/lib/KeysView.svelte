<script lang="ts">
  // The multi editor's key range view: a row per part with its range as a
  // bar over a keyboard, which stays at the bottom of the pane. Drag either
  // end of a bar (the grips) to move that end, the bar itself to move the
  // whole range, or click the empty track to bring the nearer end there. The
  // Low and High boxes either side take a typed note too (Enter or a
  // double-click), or ↑ ↓ like any number.
  //
  // Keyboard: Tab and ← → move through the controls, row by row; on a grip
  // ↑ ↓ move that end a semitone (Shift an octave), Home and End to the ends.
  import type { MultiView, ParamInfo } from "./api";
  import NumCell from "./NumCell.svelte";
  import { noteName } from "./format";
  import { rove } from "./keys";
  import { modKey } from "./prefs.svelte";
  import { throttled } from "./throttle";

  let {
    view,
    valueOf,
    write,
    info,
    editable,
    sel,
    chosen,
    onselect,
    usedOnly,
  }: {
    view: MultiView;
    valueOf: (p: number, id: string) => number | undefined;
    write: (p: number, id: string, v: number) => void;
    info: (id: string) => ParamInfo | undefined;
    editable: boolean;
    sel: number;
    chosen: Set<number>;
    onselect: (e: MouseEvent | FocusEvent | null, i: number) => void;
    usedOnly: boolean;
  } = $props();

  const LO_ID = "multi_part.low_key";
  const HI_ID = "multi_part.high_key";

  // The keyboard, A-1 (21) to G8 (127), in white-key units: black keys sit
  // over the line between two white keys, as on the program editor's strip.
  const LOW = 21;
  const HIGH = 127;
  const isBlack = (n: number) => [1, 3, 6, 8, 10].includes(n % 12);
  const notes = [...Array(HIGH - LOW + 1).keys()].map((i) => LOW + i);
  const whites = notes.filter((n) => !isBlack(n));
  const W = whites.length;
  function keyX(n: number): [number, number] {
    const w = whites.filter((m) => m < n).length;
    return isBlack(n) ? [w - 0.3, 0.6] : [w, 1];
  }
  /** Left and right edge of a key, in % of the track. */
  const left = (n: number) => (keyX(n)[0] / W) * 100;
  const right = (n: number) => ((keyX(n)[0] + keyX(n)[1]) / W) * 100;
  const centres = notes.map((n) => keyX(n)[0] + keyX(n)[1] / 2);
  const cs = notes.filter((n) => n % 12 === 0);

  const rows = $derived(view.parts.map((p, i) => ({ p, i })).filter(({ p, i }) => !usedOnly || p.program || i === sel));

  // A drag in progress, shown at once; the writes follow at most 20 a second.
  type What = "lo" | "hi" | "both";
  let drag = $state<{ i: number; what: What; lo: number; hi: number; lo0: number; hi0: number; from: number } | null>(
    null,
  );
  let track: DOMRect | null = null;
  let seq = 0;
  const sender = throttled(flush);

  const lo = (i: number) => (drag?.i === i ? drag.lo : valueOf(i, LO_ID));
  const hi = (i: number) => (drag?.i === i ? drag.hi : valueOf(i, HI_ID));

  /** The key nearest the pointer. */
  function noteAt(x: number): number {
    if (!track) return LOW;
    const u = ((x - track.left) / track.width) * W;
    let best = 0;
    for (let k = 1; k < centres.length; k++) if (Math.abs(centres[k] - u) < Math.abs(centres[best] - u)) best = k;
    return notes[best];
  }

  /** Writes the dragged range: the end moving outwards first, so the low key
   *  never passes the high one on the way. */
  function flush() {
    if (!drag) return;
    const { i, lo: l, hi: h } = drag;
    const was = [valueOf(i, LO_ID), valueOf(i, HI_ID)];
    const both: [string, number, number | undefined][] = [
      [LO_ID, l, was[0]],
      [HI_ID, h, was[1]],
    ];
    if (h > (was[1] ?? h)) both.reverse();
    for (const [id, v, old] of both) if (v !== old) write(i, id, v);
  }

  function down(e: PointerEvent, i: number, what: What | null) {
    const l = valueOf(i, LO_ID);
    const h = valueOf(i, HI_ID);
    // ⌘-click and Shift-click select parts (the row's handler).
    if (!editable || l === undefined || h === undefined || e.button !== 0 || modKey(e) || e.shiftKey) return;
    const el = e.currentTarget as HTMLElement;
    track = el.closest(".track")!.getBoundingClientRect();
    const n = noteAt(e.clientX);
    // The empty track: the nearer end comes to the pointer.
    const w: What = what ?? (Math.abs(n - l) <= Math.abs(n - h) ? "lo" : "hi");
    try {
      el.setPointerCapture(e.pointerId);
    } catch {
      // Dragging still works without capture.
    }
    drag = { i, what: w, lo: l, hi: h, lo0: l, hi0: h, from: n };
    if (!what) move(e);
  }

  function move(e: PointerEvent) {
    if (!drag || !track) return;
    const n = noteAt(e.clientX);
    let { lo: l, hi: h } = drag;
    if (drag.what === "lo") l = Math.min(n, drag.hi);
    else if (drag.what === "hi") h = Math.max(n, drag.lo);
    else {
      const d = Math.min(HIGH - drag.hi0, Math.max(LOW - drag.lo0, n - drag.from));
      l = drag.lo0 + d;
      h = drag.hi0 + d;
    }
    if (l !== drag.lo || h !== drag.hi) {
      drag.lo = l;
      drag.hi = h;
      sender.push(++seq);
    }
  }

  function up() {
    if (!drag) return;
    sender.done(seq);
    drag = null;
    track = null;
  }

  function gripKey(e: KeyboardEvent, i: number, end: "lo" | "hi") {
    const l = valueOf(i, LO_ID);
    const h = valueOf(i, HI_ID);
    if (!editable || l === undefined || h === undefined || e.altKey || e.metaKey || e.ctrlKey) return;
    const from = end === "lo" ? l : h;
    const step = e.shiftKey ? 12 : 1;
    const to: Record<string, number> = {
      ArrowUp: from + step,
      ArrowDown: from - step,
      Home: end === "lo" ? LOW : l,
      End: end === "hi" ? HIGH : h,
    };
    if (!(e.key in to)) return;
    e.preventDefault();
    const v = end === "lo" ? Math.max(LOW, Math.min(h, to[e.key])) : Math.min(HIGH, Math.max(l, to[e.key]));
    if (v !== from) write(i, end === "lo" ? LO_ID : HI_ID, v);
  }

  const on = (fn: (v: number) => void) => (editable ? fn : undefined);
</script>

<div class="keysview" style:--route="var(--mta-lime)">
  <div class="rows" use:rove={{ tab: "each" }} role="group" aria-label="Parts">
    {#each rows as { p, i } (i)}
      {@const l = lo(i)}
      {@const h = hi(i)}
      <div
        class="row"
        class:selected={chosen.has(i)}
        class:current={i === sel}
        class:empty={!p.program}
        onpointerdown={(e) => onselect(e, i)}
        onfocusin={(e) => (e.target as HTMLElement).matches(":focus-visible") && onselect(e, i)}
        role="group"
        aria-label="Part {i + 1}"
      >
        <button class="num" aria-pressed={chosen.has(i)} aria-label="Select part {i + 1}">{i + 1}</button>
        <span class="prog" title={p.program ?? "No program"}>{p.program ?? "—"}</span>
        <span class="box">
          {#if info(LO_ID)}<NumCell info={info(LO_ID)!} value={l} onchange={on((v) => write(i, LO_ID, Math.min(v, h ?? v)))} />{/if}
        </span>
        <span class="track" class:editable onpointerdown={(e) => down(e, i, null)} onpointermove={move} onpointerup={up} onpointercancel={up} role="presentation">
          {#each cs as c}<span class="c-line" style:left="{left(c)}%"></span>{/each}
          {#if l !== undefined && h !== undefined && h >= l}
            <span
              class="bar"
              class:dragging={drag?.i === i}
              style:left="{left(l)}%"
              style:width="{right(h) - left(l)}%"
              title="{noteName(l)} – {noteName(h)}"
              onpointerdown={(e) => {
                e.stopPropagation();
                onselect(e, i);
                down(e, i, "both");
              }}
              onpointermove={move}
              onpointerup={up}
              onpointercancel={up}
              role="presentation"
            >
              {#each ["lo", "hi"] as const as end}
                <span
                  class="grip {end}"
                  role="slider"
                  aria-label="Part {i + 1} {end === 'lo' ? 'low' : 'high'} key"
                  aria-valuemin={LOW}
                  aria-valuemax={HIGH}
                  aria-valuenow={end === "lo" ? l : h}
                  aria-valuetext={noteName(end === "lo" ? l : h)}
                  aria-disabled={!editable}
                  tabindex={editable ? 0 : -1}
                  onpointerdown={(e) => {
                    e.stopPropagation();
                    onselect(e, i);
                    down(e, i, end);
                  }}
                  onpointermove={move}
                  onpointerup={up}
                  onpointercancel={up}
                  onkeydown={(e) => gripKey(e, i, end)}
                ></span>
              {/each}
            </span>
          {/if}
        </span>
        <span class="box">
          {#if info(HI_ID)}<NumCell info={info(HI_ID)!} value={h} onchange={on((v) => write(i, HI_ID, Math.max(v, l ?? v)))} />{/if}
        </span>
      </div>
    {/each}
    {#if !rows.length}<p class="muted none">No part has a program.</p>{/if}
  </div>

  <!-- The keyboard, under the tracks; the current part's keys are lit. -->
  <div class="row keyboard-row" aria-hidden="true">
    <span></span>
    <span class="kb-label">Part {sel + 1}: {noteName(lo(sel) ?? LOW)} – {noteName(hi(sel) ?? HIGH)}</span>
    <span class="kb-wrap">
      <svg viewBox="0 0 {W} 10" preserveAspectRatio="none" class="keyboard">
        {#each whites as n}
          {@const lit = (lo(sel) ?? HIGH + 1) <= n && n <= (hi(sel) ?? LOW - 1)}
          <rect x={keyX(n)[0]} y="0" width="1" height="10" class="white" class:lit />
        {/each}
        {#each notes.filter(isBlack) as n}
          {@const lit = (lo(sel) ?? HIGH + 1) <= n && n <= (hi(sel) ?? LOW - 1)}
          <rect x={keyX(n)[0]} y="0" width={keyX(n)[1]} height="6.2" class="black" class:lit />
        {/each}
      </svg>
      {#each cs as c}<span class="c-name" style:left="{left(c)}%">{noteName(c)}</span>{/each}
    </span>
    <span></span>
  </div>
</div>

<style>
  /* Columns: part number, program, low box, track, high box. The keyboard
     row uses the same, so it lines up with the tracks. */
  .keysview {
    --cols: 28px 150px 48px minmax(240px, 1fr) 48px;
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    font-size: 11px;
  }
  .rows {
    flex: 1 1 auto;
  }
  .row {
    display: grid;
    grid-template-columns: var(--cols);
    align-items: center;
    column-gap: 6px;
    height: 24px;
    border-bottom: 1px solid var(--rule-soft);
  }
  .row.selected {
    background: var(--select-soft);
  }
  .row.empty:not(.selected) > :not(.num) {
    opacity: 0.55;
  }
  .num {
    height: 18px;
    padding: 0;
    border: none;
    background: var(--mta-purple);
    color: #fff;
    font-size: 10px;
    font-weight: 700;
  }
  .row.current .num {
    outline: 2px solid var(--text);
    outline-offset: -2px;
  }
  .prog {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 700;
  }
  .row.empty .prog {
    font-weight: 400;
    color: var(--muted);
  }
  .box {
    display: flex;
    height: 18px;
  }
  .box :global(> *) {
    flex: 1;
  }
  /* The track: a C line every octave, and the range as a bar. Room either
     side for the grips at the ends of the keyboard. */
  .track {
    position: relative;
    height: 100%;
    margin: 0 9px;
    touch-action: none;
  }
  .track.editable {
    cursor: pointer;
  }
  .c-line {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    background: var(--rule-soft);
  }
  .bar {
    position: absolute;
    top: 5px;
    bottom: 5px;
    background: var(--route);
    border: 1px solid var(--rule);
    min-width: 2px;
  }
  .track.editable .bar {
    cursor: grab;
  }
  .bar.dragging {
    cursor: grabbing;
  }
  .grip {
    position: absolute;
    top: -4px;
    bottom: -4px;
    width: 8px;
    background: var(--text);
    border: 1px solid var(--bg);
  }
  .grip.lo {
    right: 100%;
  }
  .grip.hi {
    left: 100%;
  }
  .track.editable .grip {
    cursor: ew-resize;
  }
  .grip:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 1px;
  }
  .none {
    padding: 20px 0;
  }

  /* The keyboard stays in view at the bottom of the pane. */
  .keyboard-row {
    position: sticky;
    bottom: 0;
    z-index: 1;
    height: auto;
    padding: 6px 0 4px;
    background: var(--bg);
    border-top: 1px solid var(--rule);
    border-bottom: none;
  }
  .kb-label {
    grid-column: 2 / span 2;
    font-weight: 700;
    align-self: start;
  }
  .kb-wrap {
    position: relative;
    margin: 0 9px;
    padding-bottom: 14px;
  }
  .keyboard {
    display: block;
    width: 100%;
    height: 44px;
  }
  .white {
    fill: #fff;
    stroke: #000;
    stroke-width: 0.06;
  }
  .black {
    fill: #000;
  }
  .white.lit {
    fill: color-mix(in srgb, var(--route) 55%, #fff);
  }
  .black.lit {
    fill: color-mix(in srgb, var(--route) 60%, #000);
  }
  .c-name {
    position: absolute;
    bottom: 0;
    font-size: 9px;
    color: var(--muted);
    white-space: nowrap;
  }
</style>
