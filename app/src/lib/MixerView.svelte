<script lang="ts">
  // The multi editor's mixer view: a channel strip per part, 32 at a time,
  // with an overview of every block of 32 above them. Each strip, top to
  // bottom: part number, program, MIDI channel, output, FX bus, FX send,
  // pan, key range, mute/solo and the level fader. The same values as the
  // table view, and the same writes.
  //
  // Keyboard: Tab and ← → move through the controls, strip by strip; ↑ ↓
  // change a value, ⌥← ⌥→ move to the same control in the next strip.
  import type { MultiView, ParamInfo } from "./api";
  import Fader from "./Fader.svelte";
  import MiniKnob from "./MiniKnob.svelte";
  import NumCell from "./NumCell.svelte";
  import { rove } from "./keys";

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
    programChoices,
    assign,
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
    programChoices: (current: string | null) => string[];
    assign: (p: number, program: string) => void;
  } = $props();

  const P = "multi_part.";
  const LOW = 21;
  const HIGH = 127;
  const BLOCK = 32;

  let start = $state(0);
  const count = $derived(view.parts.length);
  // Keep the selected part in view when it moves outside the block (⌥↑/⌥↓, the table).
  $effect(() => {
    if (!usedOnly && (sel < start || sel >= start + BLOCK)) start = Math.floor(sel / BLOCK) * BLOCK;
  });
  const strips = $derived(
    usedOnly
      ? view.parts.map((p, i) => ({ p, i })).filter(({ p, i }) => p.program || i === sel)
      : view.parts.slice(start, start + BLOCK).map((p, j) => ({ p, i: start + j })),
  );
  const blocks = $derived(
    Array.from({ length: Math.ceil(count / BLOCK) }, (_, b) => ({
      from: b * BLOCK,
      parts: view.parts.slice(b * BLOCK, (b + 1) * BLOCK).map((p, j) => ({
        has: !!p.program,
        level: valueOf(b * BLOCK + j, `${P}level`) ?? 0,
        muted: valueOf(b * BLOCK + j, `${P}mute`) === 1,
      })),
    })),
  );

  const on = (fn: ((v: number) => void) | undefined) => (editable ? fn : undefined);

  /** Bottom and height of the key range in the bar, in %. */
  function span(i: number): [number, number] | null {
    const lo = valueOf(i, `${P}low_key`);
    const hi = valueOf(i, `${P}high_key`);
    if (lo === undefined || hi === undefined || hi < lo) return null;
    const y = (n: number) => ((Math.min(HIGH, Math.max(LOW, n)) - LOW) / (HIGH - LOW + 1)) * 100;
    return [y(lo), y(hi + 1) - y(lo)];
  }

  const FOCUSABLE = '[role="slider"], [role="spinbutton"], select, button:not([disabled])';

  /** ⌥← / ⌥→: the same control in the strip to the left or right. */
  function stripKey(e: KeyboardEvent) {
    if (!e.altKey || e.metaKey || e.ctrlKey || (e.key !== "ArrowLeft" && e.key !== "ArrowRight")) return;
    const el = (e.target as HTMLElement).closest<HTMLElement>(FOCUSABLE);
    const strip = el?.closest(".strip");
    if (!el || !strip) return;
    e.preventDefault();
    const index = [...strip.querySelectorAll(FOCUSABLE)].indexOf(el);
    const next = (e.key === "ArrowLeft" ? strip.previousElementSibling : strip.nextElementSibling) as HTMLElement | null;
    if (next?.classList.contains("strip")) next.querySelectorAll<HTMLElement>(FOCUSABLE)[index]?.focus();
  }
</script>

{#snippet choice(i: number, id: string)}
  {@const pi = info(P + id)}
  {@const v = valueOf(i, P + id)}
  {#if pi && editable && v !== undefined}
    <select class="lcd-select" value={v} onchange={(e) => write(i, P + id, parseInt(e.currentTarget.value))} aria-label="Part {i + 1} {pi.label}" title={pi.label}>
      {#each pi.options ?? [] as o, k}<option value={k}>{o}</option>{/each}
    </select>
  {:else}
    <span class="lcd-box" title={pi?.label}>{v === undefined || !pi ? "—" : (pi.options?.[v] ?? v)}</span>
  {/if}
{/snippet}

{#snippet toggle(i: number, id: string, head: string, lit: string)}
  {@const pi = info(P + id)}
  {@const v = valueOf(i, P + id)}
  <button
    class="tog"
    class:on={v === 1}
    style:--lit={lit}
    disabled={!editable || v === undefined}
    aria-pressed={v === 1}
    aria-label="Part {i + 1} {pi?.label ?? head}"
    onclick={() => write(i, P + id, v === 1 ? 0 : 1)}>{head}</button
  >
{/snippet}

<div class="mixer">
  {#if !usedOnly && blocks.length > 1}
    <div class="overview">
      <span class="caption">All parts</span>
      {#each blocks as b}
        <button
          class="block"
          class:on={b.from === start}
          aria-pressed={b.from === start}
          aria-label="Show parts {b.from + 1} to {b.from + BLOCK}"
          onclick={() => (start = b.from)}
        >
          <span class="bars" aria-hidden="true">
            {#each b.parts as x}
              <span class="bar" class:has={x.has} class:muted={x.muted} style:height="{x.has ? Math.max(8, x.level) : 12}%"></span>
            {/each}
          </span>
          <span class="range">{b.from + 1}–{b.from + BLOCK}</span>
        </button>
      {/each}
    </div>
  {/if}

  <div class="desk" data-pane>
    <div class="labels" aria-hidden="true">
      <span class="r-num"></span>
      <span class="r-prog"><i style:background="var(--mta-purple)"></i>Program</span>
      <span class="r-sel"><i style:background="var(--mta-grey)"></i>MIDI</span>
      <span class="r-sel"><i style:background="var(--level)"></i>Out</span>
      <span class="r-sel"><i style:background="var(--mta-orange)"></i>FX</span>
      <span class="r-knob"><i style:background="var(--mta-orange)"></i>Send</span>
      <span class="r-knob"><i style:background="var(--level)"></i>Pan</span>
      <span class="r-keys"><i style:background="var(--mta-lime)"></i>Keys</span>
      <span class="r-ms">M / S</span>
      <span class="r-fader"><i style:background="var(--level)"></i>Level</span>
      <span class="r-read"></span>
    </div>
    <div
      class="strips"
      use:rove={{ tab: "each" }}
      {@attach (el) => {
        el.addEventListener("keydown", stripKey);
        return () => el.removeEventListener("keydown", stripKey);
      }}
      role="group"
      aria-label="Parts"
    >
      {#each strips as { p, i } (i)}
        {@const range = span(i)}
        <div
          class="strip"
          class:selected={chosen.has(i)}
          class:current={i === sel}
          class:empty={!p.program}
          onfocusin={(e) => (e.target as HTMLElement).matches(":focus-visible") && onselect(e, i)}
          onpointerdown={(e) => onselect(e, i)}
          role="group"
          aria-label="Part {i + 1}"
        >
          <button class="r-num num" aria-pressed={chosen.has(i)} aria-label="Select part {i + 1}">{i + 1}</button>
          <span class="r-prog prog" title={p.program ?? "No program"}>
            <span class="name">{p.program ?? "—"}</span>
            {#if editable}
              <select value={p.program ?? ""} onchange={(e) => assign(i, e.currentTarget.value)} aria-label="Part {i + 1} program">
                {#if !p.program}<option value="" disabled>—</option>{/if}
                {#each programChoices(p.program) as name}<option value={name}>{name}</option>{/each}
              </select>
            {/if}
          </span>
          <span class="r-sel">{@render choice(i, "midi_channel")}</span>
          <span class="r-sel">{@render choice(i, "output")}</span>
          <span class="r-sel">{@render choice(i, "fx_channel")}</span>
          <span class="r-knob" style:--route="var(--mta-orange)">
            {#if info(`${P}fx_send`)}
              <MiniKnob info={info(`${P}fx_send`)!} value={valueOf(i, `${P}fx_send`)} label="Part {i + 1} FX send" onchange={on((v) => write(i, `${P}fx_send`, v))} />
            {/if}
          </span>
          <span class="r-knob" style:--route="var(--level)">
            {#if info(`${P}pan`)}
              <MiniKnob info={info(`${P}pan`)!} value={valueOf(i, `${P}pan`)} label="Part {i + 1} pan" onchange={on((v) => write(i, `${P}pan`, v))} />
            {/if}
          </span>
          <span class="r-keys keys" style:--route="var(--mta-lime)">
            {#if info(`${P}high_key`)}
              <NumCell info={info(`${P}high_key`)!} value={valueOf(i, `${P}high_key`)} onchange={on((v) => write(i, `${P}high_key`, v))} />
            {/if}
            <span class="keybar" aria-hidden="true">
              {#if range}<span style:bottom="{range[0]}%" style:height="{range[1]}%"></span>{/if}
            </span>
            {#if info(`${P}low_key`)}
              <NumCell info={info(`${P}low_key`)!} value={valueOf(i, `${P}low_key`)} onchange={on((v) => write(i, `${P}low_key`, v))} />
            {/if}
          </span>
          <span class="r-ms ms">
            {@render toggle(i, "mute", "M", "var(--mta-red)")}
            {@render toggle(i, "solo", "S", "var(--mta-green)")}
          </span>
          <span class="r-fader">
            {#if info(`${P}level`)}
              <Fader info={info(`${P}level`)!} value={valueOf(i, `${P}level`)} label="Part {i + 1} level" onchange={on((v) => write(i, `${P}level`, v))} />
            {/if}
          </span>
          <span class="r-read"><span class="lcd-box">{valueOf(i, `${P}level`) ?? "—"}</span></span>
        </div>
      {/each}
      {#if !strips.length}<p class="muted none">No part has a program.</p>{/if}
    </div>
  </div>
</div>

<style>
  /* The level colour: the scheme's accent (blue in the standard schemes). */
  .mixer {
    --level: var(--accent);
    display: flex;
    flex-direction: column;
    gap: 10px;
    /* Fills the rest of the pane (the editor is a flex column), and below
       500 px the pane scrolls instead. */
    flex: 1 1 auto;
    min-height: 500px;
  }
  .overview {
    display: flex;
    gap: 6px;
    align-items: stretch;
  }
  .caption,
  .labels {
    color: var(--muted);
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .caption {
    width: 64px;
    flex: none;
    align-self: center;
  }
  .block {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 4px;
    font-weight: 400;
    border: 1px solid var(--rule-soft);
    max-width: 240px;
  }
  .block.on {
    outline: 2px solid var(--text);
    outline-offset: -2px;
  }
  .bars {
    display: flex;
    align-items: flex-end;
    gap: 1px;
    height: 22px;
    width: 100%;
  }
  .bar {
    flex: 1;
    background: var(--rule-soft);
  }
  .bar.has {
    background: var(--level);
  }
  .bar.has.muted {
    background: var(--muted);
  }
  .block .range {
    font-size: 10px;
    font-weight: 700;
  }
  .desk {
    flex: 1;
    min-height: 0;
    display: flex;
    overflow-x: auto;
    overflow-y: hidden;
    border-top: 1px solid var(--rule);
  }
  .labels,
  .strip {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px 3px;
  }
  .labels {
    width: 64px;
    flex: none;
    padding-left: 0;
    border-right: 1px solid var(--rule);
    position: sticky;
    left: 0;
    background: var(--bg);
    z-index: 1;
  }
  .labels span {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .labels i {
    width: 3px;
    height: 12px;
    flex: none;
  }
  .labels .r-fader {
    align-items: flex-start;
  }
  /* Row heights, shared by the labels and every strip. */
  .r-num {
    height: 18px;
    flex: none;
  }
  .r-prog {
    height: 120px;
    flex: none;
  }
  .r-sel {
    height: 20px;
    flex: none;
  }
  .r-knob {
    height: 40px;
    flex: none;
  }
  .r-keys {
    height: 60px;
    flex: none;
  }
  .r-ms {
    height: 40px;
    flex: none;
  }
  .r-fader {
    flex: 1;
    min-height: 80px;
  }
  .r-read {
    height: 18px;
    flex: none;
  }
  .strips {
    display: flex;
  }
  .strip {
    width: 40px;
    flex: none;
    border-right: 1px solid var(--rule-soft);
  }
  .strip.selected {
    background: var(--surface-2);
  }
  .strip.empty > :not(.r-num) {
    opacity: 0.55;
  }
  .num {
    padding: 0;
    border: none;
    background: var(--mta-purple);
    color: #fff;
    font-size: 10px;
    font-weight: 700;
  }
  .strip.current .num {
    outline: 2px solid var(--text);
    outline-offset: -2px;
  }
  /* The scribble strip: the program name, written upwards, on a sign; a
     transparent menu over it picks the program. */
  .prog {
    position: relative;
    display: flex;
    justify-content: center;
    overflow: hidden;
    background: var(--sign-bg);
    color: var(--sign-text);
    padding: 4px 0;
  }
  .prog .name {
    writing-mode: vertical-rl;
    transform: rotate(180deg);
    white-space: nowrap;
    font-size: 11px;
    font-weight: 700;
  }
  .prog select {
    position: absolute;
    inset: 0;
    opacity: 0;
    width: 100%;
    height: 100%;
    cursor: pointer;
  }
  .prog:has(select:focus-visible) {
    outline: 2px solid var(--focus);
    outline-offset: 1px;
  }
  .lcd-select,
  .lcd-box {
    display: block;
    width: 100%;
    height: 100%;
    padding: 0;
    font-size: 10px;
    font-weight: 700;
    text-align: center;
    text-align-last: center;
    background: var(--lcd-bg);
    color: var(--lcd-fg);
    border: 1px solid var(--rule);
    appearance: none;
    -webkit-appearance: none;
    line-height: 18px;
    overflow: hidden;
    white-space: nowrap;
  }
  .lcd-select {
    cursor: pointer;
  }
  .strip .r-knob {
    display: flex;
    justify-content: center;
  }
  .keys {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .keys :global(.num) {
    min-width: 0;
    padding: 0;
    font-size: 9px;
    text-align: center;
  }
  .keybar {
    flex: 1;
    position: relative;
    background: var(--surface-2);
  }
  .keybar span {
    position: absolute;
    left: 3px;
    right: 3px;
    min-height: 2px;
    background: var(--route);
  }
  .ms {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .tog {
    flex: 1;
    padding: 0;
    font-size: 10px;
    font-weight: 700;
  }
  .tog.on {
    background: var(--lit);
    border-color: var(--lit);
    color: #fff;
  }
  .tog:disabled {
    opacity: 1;
  }
  .r-read .lcd-box {
    line-height: 16px;
  }
  .none {
    padding: 12px;
  }
</style>
