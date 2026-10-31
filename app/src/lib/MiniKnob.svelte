<script lang="ts">
  // A small knob for the mixer's strips: the arc in the route colour and the
  // value beneath. Drag up or down (Shift for fine steps); ↑/↓ step (Shift
  // ×10), Home/End the ends; Enter or a double-click to type.
  import type { ParamInfo } from "./api";
  import { onDestroy } from "svelte";
  import { linked, numberMember } from "./linked.svelte";
  import { modKey } from "./prefs.svelte";
  import { dialogs } from "./dialogs.svelte";
  import { formatPart } from "./partFormat";
  import { throttled } from "./throttle";

  let {
    info,
    value,
    label,
    onchange,
  }: { info: ParamInfo; value: number | undefined; label: string; onchange?: (v: number) => void } = $props();

  const min = $derived(info.min ?? 0);
  const max = $derived(info.max ?? 127);
  const editable = $derived(!!onchange && info.writable && value !== undefined);
  const clamp = (v: number) => Math.min(max, Math.max(min, Math.round(v)));

  let local = $state<number | null>(null);
  const shown = $derived(local ?? value);
  const live = throttled((v) => onchange?.(v));

  // Linking (linked.svelte.ts): ⌘-click adds it to the linked set; linked
  // number controls follow by the same amount.
  const member = numberMember(() => value, () => [min, max], live);
  const isLinked = $derived(linked.has(member));
  onDestroy(() => linked.remove(member));
  function link(e: MouseEvent) {
    if (!editable || !modKey(e)) return false;
    // Not also a click on the part's row or strip (selecting parts).
    e.preventDefault();
    e.stopPropagation();
    linked.toggle(member);
    return true;
  }

  const SWEEP = 270;
  const angle = (v: number) => -SWEEP / 2 + ((clamp(v) - min) / (max - min || 1)) * SWEEP;
  function arc(a0: number, a1: number, r = 15) {
    const p = (a: number) => {
      const t = ((a - 90) * Math.PI) / 180;
      return `${(r * Math.cos(t)).toFixed(2)} ${(r * Math.sin(t)).toFixed(2)}`;
    };
    const [s, e] = a0 <= a1 ? [a0, a1] : [a1, a0];
    return `M ${p(s)} A ${r} ${r} 0 ${e - s > 180 ? 1 : 0} 1 ${p(e)}`;
  }
  const zero = $derived(min < 0 && max > 0 ? 0 : min);

  let startY = 0;
  let startV = 0;
  let dragging = false;

  function down(e: PointerEvent) {
    if (link(e) || !editable || value === undefined || e.button !== 0) return;
    linked.begin(member);
    try {
      (e.currentTarget as Element).setPointerCapture(e.pointerId);
    } catch {
      // Dragging still works without capture.
    }
    dragging = true;
    startY = e.clientY;
    startV = value;
    local = value;
  }

  function move(e: PointerEvent) {
    if (!dragging) return;
    const next = clamp(startV + ((startY - e.clientY) * (e.shiftKey ? 0.2 : 1) * (max - min)) / 140);
    if (next !== local) {
      local = next;
      live.push(next);
      linked.follow(member, next - startV, next);
    }
  }

  function up() {
    if (!dragging) return;
    dragging = false;
    if (local !== null && local !== startV) live.done(local);
    local = null;
    linked.end(member);
  }

  function key(e: KeyboardEvent) {
    if (!editable || value === undefined || e.altKey || e.metaKey || e.ctrlKey) return;
    const from = local ?? value;
    const to: Record<string, number> = {
      ArrowUp: from + (e.shiftKey ? 10 : 1),
      ArrowDown: from - (e.shiftKey ? 10 : 1),
      Home: min,
      End: max,
    };
    if (e.key === "Enter") {
      e.preventDefault();
      void type();
    } else if (e.key in to) {
      e.preventDefault();
      if (local === null) {
        startV = value;
        linked.begin(member);
      }
      local = clamp(to[e.key]);
      live.push(local);
      linked.follow(member, local - startV, local);
    }
  }

  function keyUp() {
    if (local === null || dragging) return;
    live.done(local);
    local = null;
    linked.end(member);
  }

  async function type() {
    if (!editable) return;
    const r = await dialogs.ask(label, [{ label: `${min} to ${max}`, value: String(value ?? "") }], "Set");
    const v = r ? parseInt(r[0]) : NaN;
    if (Number.isNaN(v)) return;
    onchange?.(clamp(v));
    linked.once(member, clamp(v) - (value ?? 0), clamp(v));
  }
</script>

<div
  class="knob"
  class:editable
  class:linked={isLinked}
  role="slider"
  aria-label={label}
  aria-valuemin={min}
  aria-valuemax={max}
  aria-valuenow={shown}
  aria-valuetext={shown === undefined ? undefined : formatPart(info, shown)}
  aria-disabled={!editable}
  tabindex={editable ? 0 : -1}
  title={label}
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={up}
  ondblclick={type}
  onkeydown={key}
  onkeyup={keyUp}
>
  <svg viewBox="-20 -20 40 40" aria-hidden="true">
    <path d={arc(-SWEEP / 2, SWEEP / 2)} class="track" />
    {#if shown !== undefined}
      {#if shown !== zero}<path d={arc(angle(zero), angle(shown))} class="value" />{/if}
      <line x1="0" y1="-3" x2="0" y2="-12" transform="rotate({angle(shown)})" class="pointer" />
    {/if}
  </svg>
  <span class="text">{shown === undefined ? "—" : formatPart(info, shown)}</span>
</div>

<style>
  .knob {
    display: flex;
    flex-direction: column;
    align-items: center;
    touch-action: none;
  }
  .knob.editable {
    cursor: ns-resize;
  }
  svg {
    width: 28px;
    height: 28px;
  }
  .track {
    fill: none;
    stroke: var(--rule-soft);
    stroke-width: 1.5;
  }
  .value {
    fill: none;
    stroke: var(--route);
    stroke-width: 5;
  }
  .pointer {
    stroke: var(--text);
    stroke-width: 2;
  }
  .text {
    font-size: 9px;
    line-height: 11px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .knob.linked {
    outline: 2px dashed var(--accent);
    outline-offset: 0;
  }
  .knob:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 0;
  }
</style>
