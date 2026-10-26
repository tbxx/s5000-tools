<script lang="ts">
  // Envelope graph. Amp and filter: attack, decay, sustain, release. Aux: four
  // rates and four levels. All are 0–100. When `onchange` is given the
  // breakpoints can be dragged; writes go through the throttled live-write
  // path, like the knobs.
  import type { ParamInfo } from "./api";
  import { throttled, type Throttled } from "./throttle";

  let {
    kind,
    get,
    catalog,
    onchange,
  }: {
    kind: "amp" | "filter" | "aux";
    /** Current value of a parameter ref ("id" or "id#n"). */
    get: (ref: string) => number | undefined;
    catalog: Map<string, ParamInfo>;
    onchange?: (ref: string, v: number) => void;
  } = $props();

  // Drawing area, in SVG units (the graph scales to its box).
  const W = 280;
  const H = 86;
  const PAD = 7;
  const IW = W - 2 * PAD;
  const IH = H - 2 * PAD;
  const X = (u: number) => PAD + u * IW; // u: 0–1 across
  const Y = (level: number) => PAD + IH - (level / 100) * IH; // level: 0–100

  /** Width of one timed segment at full value, as a fraction of the width. */
  const SEG = $derived(kind === "aux" ? 0.21 : 0.27);
  /** The held part (sustain) between the timed segments. */
  const HOLD = $derived(kind === "aux" ? 0.12 : 0.17);

  const auxRate = [1, 2, 3, 4].map((n) => `keygroup.aux_env_rate#${n}`);
  const auxLevel = [1, 2, 3, 4].map((n) => `keygroup.aux_env_level#${n}`);
  const adsr = $derived(["attack", "decay", "sustain", "release"].map((f) => `keygroup.${kind}_env_${f}`));

  // Values being dragged, shown before the write comes back.
  let local = $state<Record<string, number>>({});
  const val = (ref: string) => local[ref] ?? get(ref);

  /** A breakpoint: where it is and what dragging it changes. */
  interface Point {
    x: number;
    y: number;
    /** Ref set by horizontal drag, and the x where its segment starts. */
    h?: { ref: string; from: number };
    /** Ref set by vertical drag. */
    v?: string;
  }

  const points = $derived.by((): Point[] | null => {
    const seg = (v: number) => (v / 100) * SEG;
    if (kind === "aux") {
      const r = auxRate.map(val);
      const l = auxLevel.map(val);
      if ([...r, ...l].some((v) => v === undefined)) return null;
      const x1 = seg(r[0]!), x2 = x1 + seg(r[1]!), x3 = x2 + seg(r[2]!), x4 = x3 + HOLD;
      return [
        { x: 0, y: 0 },
        { x: x1, y: l[0]!, h: { ref: auxRate[0], from: 0 }, v: auxLevel[0] },
        { x: x2, y: l[1]!, h: { ref: auxRate[1], from: x1 }, v: auxLevel[1] },
        { x: x3, y: l[2]!, h: { ref: auxRate[2], from: x2 }, v: auxLevel[2] },
        { x: x4, y: l[2]! },
        { x: x4 + seg(r[3]!), y: l[3]!, h: { ref: auxRate[3], from: x4 }, v: auxLevel[3] },
      ];
    }
    const [attack, decay, sustain, release] = adsr;
    const [a, d, s, r] = adsr.map(val);
    if ([a, d, s, r].some((v) => v === undefined)) return null;
    const x1 = seg(a!), x2 = x1 + seg(d!), x3 = x2 + HOLD;
    return [
      { x: 0, y: 0 },
      { x: x1, y: 100, h: { ref: attack, from: 0 } },
      { x: x2, y: s!, h: { ref: decay, from: x1 }, v: sustain },
      { x: x3, y: s!, v: sustain },
      { x: x3 + seg(r!), y: 0, h: { ref: release, from: x3 } },
    ];
  });

  const path = $derived(points?.map((p, i) => `${i ? "L" : "M"} ${X(p.x).toFixed(1)} ${Y(p.y).toFixed(1)}`).join(" ") ?? "");

  const writable = (ref: string) => !!catalog.get(ref.split("#")[0])?.writable;
  const editable = $derived(!!onchange && points !== null);
  const draggable = (p: Point) => editable && ((p.h && writable(p.h.ref)) || (p.v && writable(p.v)));

  function label(ref: string) {
    const [id, n] = ref.split("#");
    const s = (catalog.get(id)?.label ?? id).replace(/^(Amp|Filter|Aux) env /, "");
    return s.charAt(0).toUpperCase() + s.slice(1) + (n ? ` ${n}` : "");
  }

  // Dragging one breakpoint.
  let svg: SVGSVGElement;
  let drag = $state<{ point: number } | null>(null);
  let hover = $state<number | null>(null);
  const writers = new Map<string, Throttled>();
  function writer(ref: string) {
    let w = writers.get(ref);
    if (!w) writers.set(ref, (w = throttled((v) => onchange?.(ref, v))));
    return w;
  }

  /** Pointer position in drawing units. */
  function at(e: PointerEvent): [number, number] {
    const m = svg.getScreenCTM();
    if (!m) return [0, 0];
    const p = new DOMPoint(e.clientX, e.clientY).matrixTransform(m.inverse());
    return [p.x, p.y];
  }

  const clamp = (v: number) => Math.max(0, Math.min(100, Math.round(v)));

  function down(e: PointerEvent, i: number) {
    if (!points || !draggable(points[i])) return;
    e.preventDefault();
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    drag = { point: i };
  }

  function move(e: PointerEvent) {
    if (!drag || !points) return;
    const p = points[drag.point];
    const [x, y] = at(e);
    const set = (ref: string, v: number) => {
      if (!writable(ref) || val(ref) === v) return;
      local[ref] = v;
      writer(ref).push(v);
    };
    if (p.h) set(p.h.ref, clamp((((x - PAD) / IW - p.h.from) / SEG) * 100));
    if (p.v) set(p.v, clamp(((PAD + IH - y) / IH) * 100));
  }

  function up() {
    if (!drag || !points) return;
    const p = points[drag.point];
    for (const ref of [p.h?.ref, p.v]) if (ref && ref in local) writer(ref).done(local[ref]);
    drag = null;
    local = {};
  }

  /** What the value readout shows: the point being dragged, else the one hovered. */
  const readout = $derived.by(() => {
    const i = drag?.point ?? hover;
    const p = i === null ? undefined : points?.[i];
    if (!p) return "";
    return [p.h?.ref, p.v]
      .filter((r): r is string => !!r)
      .map((r) => `${label(r)} ${val(r) ?? "—"}`)
      .join("  ·  ");
  });
</script>

<div class="env" class:editable>
  <svg
    bind:this={svg}
    viewBox="0 0 {W} {H}"
    class="graph"
    role="group"
    aria-label="{kind} envelope"
    onpointermove={move}
    onpointerup={up}
    onpointercancel={up}
  >
    <line x1={PAD} y1={Y(0)} x2={W - PAD} y2={Y(0)} class="axis" />
    <line x1={PAD} y1={Y(100)} x2={W - PAD} y2={Y(100)} class="grid" />
    {#if points}
      <path d="{path} L {X(points[points.length - 1].x).toFixed(1)} {Y(0)} L {X(0)} {Y(0)} Z" class="fill" />
      <path d={path} class="line" />
      {#each points as p, i}
        {#if draggable(p)}
          <rect
            x={X(p.x) - 3}
            y={Y(p.y) - 3}
            width="6"
            height="6"
            class="handle"
            class:active={drag?.point === i || (drag === null && hover === i)}
          />
          <rect
            x={X(p.x) - 8}
            y={Y(p.y) - 8}
            width="16"
            height="16"
            class="hit"
            class:h={p.h && !p.v}
            class:v={p.v && !p.h}
            role="slider"
            tabindex="-1"
            aria-label={[p.h?.ref, p.v].filter(Boolean).map((r) => label(r!)).join(", ")}
            aria-valuenow={p.h ? val(p.h.ref) : p.v ? val(p.v) : undefined}
            onpointerdown={(e) => down(e, i)}
            onpointerenter={() => (hover = i)}
            onpointerleave={() => hover === i && (hover = null)}
          />
        {/if}
      {/each}
    {:else}
      <text x={W / 2} y={H / 2 + 4}>—</text>
    {/if}
  </svg>
  <div class="readout" class:hint={!readout}>{readout || (editable ? "Drag the points to edit" : "")}</div>
</div>

<style>
  .env {
    display: flex;
    flex-direction: column;
    width: 100%;
    max-width: 320px;
    gap: 2px;
  }
  .graph {
    width: 100%;
    height: auto;
    aspect-ratio: 280 / 86;
    display: block;
    border: 1px solid var(--rule-soft);
    background: var(--surface);
    touch-action: none;
    overflow: visible;
  }
  .axis {
    stroke: var(--rule);
    stroke-width: 1;
  }
  .grid {
    stroke: var(--rule-soft);
    stroke-width: 1;
    stroke-dasharray: 2 3;
  }
  .fill {
    fill: var(--route);
    opacity: 0.12;
  }
  .line {
    fill: none;
    stroke: var(--route);
    stroke-width: 2;
    stroke-linejoin: miter;
  }
  .handle {
    fill: var(--surface);
    stroke: var(--text);
    stroke-width: 1;
  }
  .handle.active {
    fill: var(--text);
  }
  .hit {
    fill: transparent;
    cursor: move;
  }
  .hit.h {
    cursor: ew-resize;
  }
  .hit.v {
    cursor: ns-resize;
  }
  text {
    fill: var(--muted);
    font-size: 12px;
    text-anchor: middle;
  }
  .readout {
    min-height: 14px;
    font-size: 10px;
    font-weight: 700;
    color: var(--muted);
    text-align: center;
    font-variant-numeric: tabular-nums;
  }
  .readout:not(.hint) {
    color: var(--text);
  }
  .hint {
    font-weight: 400;
  }
</style>
