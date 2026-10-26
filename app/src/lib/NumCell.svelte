<script lang="ts">
  // A compact number for dense tables (the multi editor's parts): a square
  // readout with an optional value bar along its bottom edge in the route
  // colour. Editable when `onchange` is given: drag up or down (Shift for
  // fine steps), arrow keys step (Shift ×10), double-click to type. Writes go
  // through the shared live-write throttle, as the knobs' do.
  import type { ParamInfo } from "./api";
  import { dialogs } from "./dialogs.svelte";
  import { formatPart } from "./partFormat";
  import { throttled } from "./throttle";

  let {
    info,
    value,
    onchange,
    bar = false,
  }: {
    info: ParamInfo;
    value: number | undefined;
    onchange?: (v: number) => void;
    bar?: boolean;
  } = $props();

  const min = $derived(info.min ?? 0);
  const max = $derived(info.max ?? 127);
  const editable = $derived(!!onchange && info.writable && value !== undefined);
  const clamp = (v: number) => Math.min(max, Math.max(min, v));

  let local = $state<number | null>(null);
  const shown = $derived(local ?? value);
  const live = throttled((v) => onchange?.(v));

  // The bar grows from 0 for bipolar ranges (pan, tuning), from the left otherwise.
  const frac = (v: number) => (max > min ? (clamp(v) - min) / (max - min) : 0);
  const fill = $derived.by(() => {
    if (shown === undefined) return null;
    const zero = frac(min < 0 && max > 0 ? 0 : min);
    const at = frac(shown);
    return { left: Math.min(zero, at) * 100, width: Math.max(Math.abs(at - zero) * 100, 0) };
  });

  let startY = 0;
  let startV = 0;

  function down(e: PointerEvent) {
    if (!editable || value === undefined || e.button !== 0) return;
    try {
      (e.currentTarget as Element).setPointerCapture(e.pointerId);
    } catch {
      // Without capture a release outside the cell is missed; dragging still works.
    }
    startY = e.clientY;
    startV = value;
    local = value;
  }

  function move(e: PointerEvent) {
    if (local === null) return;
    const scale = e.shiftKey ? 0.2 : 1;
    // One full range per 160 px, but never slower than one step per 4 px.
    const perPx = Math.max((max - min) / 160, 0.25) * scale;
    const next = clamp(Math.round(startV + (startY - e.clientY) * perPx));
    if (next !== local) {
      local = next;
      live.push(next);
    }
  }

  function up() {
    if (local === null) return;
    if (local !== startV) live.done(local);
    local = null;
  }

  function key(e: KeyboardEvent) {
    if (!editable || value === undefined) return;
    const step = e.key === "ArrowUp" ? 1 : e.key === "ArrowDown" ? -1 : 0;
    if (step) {
      e.preventDefault();
      const next = clamp((local ?? value) + step * (e.shiftKey ? 10 : 1));
      local = next;
      live.push(next);
    } else if (e.key === "Enter") {
      e.preventDefault();
      void type();
    }
  }

  function keyUp() {
    if (local === null) return;
    live.done(local);
    local = null;
  }

  async function type() {
    if (!editable) return;
    const r = await dialogs.ask(info.label, [{ label: `${min} to ${max}`, value: String(value ?? "") }], "Set");
    const v = r ? parseInt(r[0]) : NaN;
    if (!Number.isNaN(v)) onchange?.(clamp(v));
  }
</script>

<span
  class="num"
  class:editable
  class:unknown={shown === undefined}
  class:dragging={local !== null}
  role="spinbutton"
  tabindex={editable ? 0 : -1}
  aria-label={info.label}
  aria-valuemin={min}
  aria-valuemax={max}
  aria-valuenow={shown}
  aria-readonly={!editable}
  title={info.notes ?? info.label}
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={up}
  onkeydown={key}
  onkeyup={keyUp}
  ondblclick={type}
>
  {shown === undefined ? "—" : formatPart(info, shown)}
  {#if bar && fill}<span class="fill" style:left="{fill.left}%" style:width="{fill.width}%"></span>{/if}
</span>

<style>
  .num {
    position: relative;
    display: inline-block;
    box-sizing: border-box;
    width: 100%;
    min-width: 34px;
    padding: 0 4px 1px;
    font-size: 11px;
    line-height: 16px;
    text-align: right;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    border: 1px solid transparent;
    touch-action: none;
  }
  .num.editable {
    cursor: ns-resize;
    border-color: var(--rule-soft);
    background: var(--field-bg);
  }
  .num.editable:hover,
  .num.dragging {
    border-color: var(--rule);
  }
  .num:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
  }
  .unknown {
    color: var(--muted);
    text-align: center;
  }
  .fill {
    position: absolute;
    bottom: 0;
    height: 2px;
    background: var(--route);
  }
</style>
