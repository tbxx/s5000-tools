<script lang="ts">
  // A vertical fader for the mixer view: click or drag anywhere on it to set
  // the level. ↑/↓ step (Shift ×10), Page Up/Down a tenth of the range,
  // Home/End the ends, Enter to type. Writes go through the live-write
  // throttle. The fill is the level colour (--level).
  import type { ParamInfo } from "./api";
  import { onDestroy } from "svelte";
  import { linked, numberMember } from "./linked.svelte";
  import { modKey } from "./prefs.svelte";
  import { dialogs } from "./dialogs.svelte";
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
  const pct = $derived(shown === undefined || max <= min ? 0 : ((shown - min) / (max - min)) * 100);
  const live = throttled((v) => onchange?.(v));
  let rect: DOMRect | null = null;
  let startV = 0;

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

  function at(e: PointerEvent) {
    if (!rect) return;
    const next = clamp(min + ((rect.bottom - e.clientY) / rect.height) * (max - min));
    if (next !== local) {
      local = next;
      live.push(next);
      linked.follow(member, next - startV, next);
    }
  }

  function down(e: PointerEvent) {
    if (link(e) || !editable || value === undefined || e.button !== 0) return;
    startV = value;
    linked.begin(member);
    const el = e.currentTarget as HTMLElement;
    try {
      el.setPointerCapture(e.pointerId);
    } catch {
      // Without capture a release outside is missed; dragging still works.
    }
    rect = el.querySelector(".track")!.getBoundingClientRect();
    local = value ?? null;
    at(e);
  }

  function up() {
    if (local !== null && rect) live.done(local);
    if (rect) linked.end(member);
    rect = null;
    local = null;
  }

  function key(e: KeyboardEvent) {
    if (!editable || value === undefined || e.altKey || e.metaKey || e.ctrlKey) return;
    const coarse = Math.max(1, Math.round((max - min) / 10));
    const from = local ?? value;
    const to: Record<string, number> = {
      ArrowUp: from + (e.shiftKey ? 10 : 1),
      ArrowDown: from - (e.shiftKey ? 10 : 1),
      PageUp: from + coarse,
      PageDown: from - coarse,
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
    if (local === null || rect) return;
    live.done(local);
    local = null;
    linked.end(member);
  }

  async function type() {
    const r = await dialogs.ask(label, [{ label: `${min} to ${max}`, value: String(value ?? "") }], "Set");
    const v = r ? parseInt(r[0]) : NaN;
    if (Number.isNaN(v)) return;
    onchange?.(clamp(v));
    linked.once(member, clamp(v) - (value ?? 0), clamp(v));
  }
</script>

<div
  class="fader"
  class:editable
  class:linked={isLinked}
  role="slider"
  aria-label={label}
  aria-orientation="vertical"
  aria-valuemin={min}
  aria-valuemax={max}
  aria-valuenow={shown}
  aria-disabled={!editable}
  tabindex={editable ? 0 : -1}
  onpointerdown={down}
  onpointermove={(e) => rect && at(e)}
  onpointerup={up}
  onpointercancel={up}
  ondblclick={() => editable && type()}
  onkeydown={key}
  onkeyup={keyUp}
>
  <span class="track">
    <span class="fill" style:height="{pct}%"></span>
    {#if shown !== undefined}<span class="cap" style:bottom="{pct}%"></span>{/if}
  </span>
</div>

<style>
  .fader {
    position: relative;
    height: 100%;
    min-height: 90px;
    touch-action: none;
    padding: 6px 0;
  }
  .fader.editable {
    cursor: ns-resize;
  }
  .track {
    position: absolute;
    left: calc(50% - 2px);
    top: 6px;
    bottom: 6px;
    width: 4px;
    background: var(--surface-2);
    outline: 1px solid var(--rule-soft);
  }
  .fill {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    background: var(--level);
  }
  /* The cap: a square knob with a line across, centred on the level. */
  .cap {
    position: absolute;
    left: -10px;
    width: 24px;
    height: 12px;
    transform: translateY(50%);
    background: var(--text);
    border: 1px solid var(--rule);
    box-sizing: border-box;
  }
  .cap::after {
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    top: 4px;
    height: 2px;
    background: var(--bg);
  }
  .fader.linked {
    outline: 2px dashed var(--accent);
    outline-offset: 1px;
  }
  .fader:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
</style>
