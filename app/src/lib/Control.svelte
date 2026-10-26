<script lang="ts">
  // One parameter: a knob for numbers, an LCD box (or menu) for value lists, a
  // switch for on/off. Editable when `onchange` is given: drag the knob up or
  // down, double-click to type a value. The knob's arc is in the panel's route
  // colour (--route).
  import type { ParamInfo } from "./api";
  import { dialogs } from "./dialogs.svelte";
  import { format } from "./format";
  import { throttled } from "./throttle";

  let {
    info,
    value,
    label = info.label,
    onchange,
  }: {
    info: ParamInfo;
    value: number | undefined;
    label?: string;
    onchange?: (v: number) => void;
  } = $props();

  const min = $derived(info.min ?? 0);
  const max = $derived(info.max ?? 127);
  const kind = $derived(
    info.encoding === "bool" ? "bool" : info.encoding === "enum" && info.options ? "enum" : "knob",
  );
  const known = $derived(value !== undefined);
  const editable = $derived(!!onchange && info.writable);

  // Knob geometry: 270° sweep, centred at the top; bipolar ranges grow from 0.
  const SWEEP = 270;
  const frac = (v: number) => (max > min ? (Math.min(max, Math.max(min, v)) - min) / (max - min) : 0);
  const angle = (v: number) => -SWEEP / 2 + frac(v) * SWEEP;
  function arc(a0: number, a1: number, r = 15) {
    const p = (a: number) => {
      const t = ((a - 90) * Math.PI) / 180;
      return `${(r * Math.cos(t)).toFixed(2)} ${(r * Math.sin(t)).toFixed(2)}`;
    };
    const [s, e] = a0 <= a1 ? [a0, a1] : [a1, a0];
    return `M ${p(s)} A ${r} ${r} 0 ${e - s > 180 ? 1 : 0} 1 ${p(e)}`;
  }
  const zero = $derived(min < 0 && max > 0 ? 0 : min);

  // Dragging: one full range per 160 px; writes go through the throttled
  // live-write path (20 a second, final value on release).
  let local = $state<number | null>(null);
  const shown = $derived(local ?? value);
  let startY = 0;
  let startV = 0;
  const live = throttled((v) => onchange?.(v));

  function down(e: PointerEvent) {
    if (!editable || value === undefined) return;
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    startY = e.clientY;
    startV = value;
    local = value;
  }

  function move(e: PointerEvent) {
    if (local === null) return;
    const scale = e.shiftKey ? 0.2 : 1;
    const v = Math.round(startV + ((startY - e.clientY) * scale * (max - min)) / 160);
    const next = Math.min(max, Math.max(min, v));
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

  async function type() {
    if (!editable) return;
    const r = await dialogs.ask(info.label, [{ label: `${min} to ${max}`, value: String(value ?? "") }], "Set");
    const v = r ? parseInt(r[0]) : NaN;
    if (!Number.isNaN(v)) onchange?.(Math.min(max, Math.max(min, v)));
  }
</script>

<div class="control {kind}" class:unknown={!known} title={info.notes ?? info.label}>
  {#if label !== ""}<span class="label">{label}</span>{/if}
  {#if kind === "knob"}
    <svg
      viewBox="-20 -20 40 40"
      class:editable
      role="slider"
      aria-label={info.label}
      aria-valuemin={min}
      aria-valuemax={max}
      aria-valuenow={shown}
      tabindex="-1"
      onpointerdown={down}
      onpointermove={move}
      onpointerup={up}
      onpointercancel={up}
      ondblclick={type}
    >
      <path d={arc(-SWEEP / 2, SWEEP / 2)} class="track" />
      {#if shown !== undefined}
        <path d={arc(angle(zero), angle(shown))} class="value" />
        <line x1="0" y1="-4" x2="0" y2="-12" transform="rotate({angle(shown)})" class="pointer" />
      {/if}
    </svg>
    <span class="range"><span>{min}</span><span>{max}</span></span>
    <span class="lcd readout">{shown === undefined ? "—" : format(info, shown)}</span>
  {:else if kind === "enum"}
    {#if editable && known}
      <select class="lcd menu" value={value} onchange={(e) => onchange?.(parseInt(e.currentTarget.value))}>
        {#each info.options ?? [] as o, i}<option value={i}>{o}</option>{/each}
      </select>
    {:else}
      <span class="lcd readout wide">{value === undefined ? "—" : format(info, value)}</span>
    {/if}
  {:else}
    <button
      class="switch"
      class:on={value === 1}
      disabled={!editable || !known}
      onclick={() => onchange?.(value === 1 ? 0 : 1)}
      aria-pressed={value === 1}
    >
      {value === undefined ? "—" : value ? "ON" : "OFF"}
    </button>
  {/if}
</div>

<style>
  .control {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1px;
    width: 62px;
  }
  .control.enum {
    width: auto;
    min-width: 62px;
  }
  .unknown {
    opacity: 0.45;
  }
  .label {
    font-size: 9.5px;
    font-weight: 700;
    letter-spacing: 0.01em;
    text-align: center;
    max-width: 100%;
    line-height: 1.1;
    min-height: 2.2em;
    display: flex;
    align-items: flex-end;
    justify-content: center;
  }
  svg {
    width: 38px;
    height: 38px;
    touch-action: none;
  }
  svg.editable {
    cursor: ns-resize;
  }
  .track {
    fill: none;
    stroke: var(--rule-soft);
    stroke-width: 1;
  }
  .value {
    fill: none;
    stroke: var(--route);
    stroke-width: 3.5;
  }
  .pointer {
    stroke: var(--text);
    stroke-width: 1.5;
  }
  .range {
    display: flex;
    justify-content: space-between;
    gap: 3px;
    width: 56px;
    font-size: 7.5px;
    color: var(--muted);
    margin-top: -5px;
  }
  .readout {
    font-size: 10px;
    min-width: 42px;
    text-align: center;
    border-color: var(--rule);
  }
  .wide {
    min-width: 60px;
    max-width: 120px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .menu {
    font-size: 10px;
    padding: 0 2px;
    max-width: 120px;
    border-color: var(--rule);
  }
  .switch {
    font-size: 10px;
    padding: 1px 8px;
    min-width: 42px;
  }
  .switch.on {
    background: var(--route);
    border-color: var(--route);
    color: #fff;
  }
  .switch:disabled {
    opacity: 1;
  }
</style>
