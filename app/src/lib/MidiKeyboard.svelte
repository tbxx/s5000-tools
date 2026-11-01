<script lang="ts">
  // The keyboard window: play the sampler from the screen. Notes go out
  // through a MIDI output on this computer (an interface cabled to the
  // sampler's MIDI In); the sampler's USB link has no way to carry them.
  //
  // Click or drag across the keys (a glissando); with "Velocity from where
  // you click", the further down the key, the louder. The computer keyboard
  // plays too: A to ' are the white keys from the octave marked under the
  // keys, W E T Y U O P the black ones; Z and X change the octave, C and V
  // the velocity. Notes stop when the window loses focus or closes.
  import { onDestroy, onMount } from "svelte";
  import { SvelteMap } from "svelte/reactivity";
  import { api } from "./api";
  import { noteName } from "./format";
  import { app } from "./state.svelte";

  // The keyboard, A-1 (21) to G8 (127), the sampler's key range.
  const LOW = 21;
  const HIGH = 127;
  const isBlack = (n: number) => [1, 3, 6, 8, 10].includes(n % 12);
  const notes = [...Array(HIGH - LOW + 1).keys()].map((i) => LOW + i);
  const whites = notes.filter((n) => !isBlack(n));
  const blacks = notes.filter(isBlack);
  const W = whites.length;
  /** Left edge of a black key, in % of the keyboard. */
  const blackLeft = (n: number) => ((whites.filter((m) => m < n).length - 0.3) / W) * 100;

  // Computer keys (by position, so any layout works) and their semitone
  // above the base note.
  const KEYS: Record<string, number> = {
    KeyA: 0, KeyW: 1, KeyS: 2, KeyE: 3, KeyD: 4, KeyF: 5, KeyT: 6, KeyG: 7, KeyY: 8, KeyH: 9,
    KeyU: 10, KeyJ: 11, KeyK: 12, KeyO: 13, KeyL: 14, KeyP: 15, Semicolon: 16, Quote: 17,
  };
  const LETTERS = ["A", "W", "S", "E", "D", "F", "T", "G", "Y", "H", "U", "J", "K", "O", "L", "P", ";", "'"];

  let ports = $state<string[] | null>(null);
  const port = $derived(app.settings.midiOutput ?? "");
  const channel = $derived(app.settings.midiChannel ?? 0);
  const velocity = $derived(app.settings.midiVelocity ?? 100);
  const byHeight = $derived(app.settings.midiVelocityByHeight ?? false);
  let sustain = $state(false);
  /** The note A plays: C3 (60) to start. */
  let base = $state(60);
  let pitch = $state(0);
  let mod = $state(0);

  /** How many presses hold each sounding note (mouse, fingers, letters). */
  const down = new SvelteMap<number, number>();
  /** The note each pointer and each computer key is holding. */
  const byPointer = new Map<number, number>();
  const byKey = new Map<string, number>();

  async function refresh() {
    try {
      ports = await api.midiOutputs();
    } catch (e) {
      ports = [];
      app.fail(e);
    }
  }

  function send(msg: number[]) {
    if (!port) return;
    api.midiSend(port, msg).catch((e) => app.fail(e));
  }

  function press(note: number, vel = velocity) {
    if (note < 0 || note > 127) return;
    const n = down.get(note) ?? 0;
    down.set(note, n + 1);
    if (n === 0) send([0x90 | channel, note, Math.max(1, Math.min(127, vel))]);
  }

  function lift(note: number | undefined) {
    if (note === undefined) return;
    const n = down.get(note) ?? 0;
    if (n <= 1) {
      down.delete(note);
      if (n === 1) send([0x80 | channel, note, 64]);
    } else down.set(note, n - 1);
  }

  /** Stops everything: every note, the pedal, the wheels back to rest. */
  function silence() {
    down.clear();
    byPointer.clear();
    byKey.clear();
    if (sustain) send([0xb0 | channel, 64, 0]);
    sustain = false;
    if (pitch !== 0) send([0xe0 | channel, 0, 64]);
    pitch = 0;
    void api.midiReleaseAll().catch(() => {});
  }

  function setPort(name: string) {
    silence();
    app.save({ midiOutput: name || undefined });
  }

  function setChannel(ch: number) {
    silence();
    app.save({ midiChannel: ch });
  }

  const setVelocity = (v: number) => app.save({ midiVelocity: Math.max(1, Math.min(127, Math.round(v))) });

  function toggleSustain() {
    sustain = !sustain;
    send([0xb0 | channel, 64, sustain ? 127 : 0]);
  }

  // The keys: the note under a pointer, and how loud (by height on the key).
  function noteAt(x: number, y: number): [number, number] | null {
    const el = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-note]");
    if (!el) return null;
    const r = el.getBoundingClientRect();
    const vel = byHeight ? Math.round(1 + (126 * (y - r.top)) / Math.max(1, r.height)) : velocity;
    return [Number(el.dataset.note), vel];
  }

  function pointerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    const hit = noteAt(e.clientX, e.clientY);
    if (!hit) return;
    e.preventDefault();
    try {
      (e.currentTarget as Element).setPointerCapture(e.pointerId);
    } catch {
      // Still plays; dragging out of the window just won't follow.
    }
    byPointer.set(e.pointerId, hit[0]);
    press(...hit);
  }

  function pointerMove(e: PointerEvent) {
    const was = byPointer.get(e.pointerId);
    if (was === undefined) return;
    const hit = noteAt(e.clientX, e.clientY);
    if (hit?.[0] === was) return;
    lift(was);
    if (hit) {
      byPointer.set(e.pointerId, hit[0]);
      press(...hit);
    } else byPointer.set(e.pointerId, -1);
  }

  function pointerUp(e: PointerEvent) {
    const was = byPointer.get(e.pointerId);
    byPointer.delete(e.pointerId);
    if (was !== undefined && was >= 0) lift(was);
  }

  // The computer keyboard. Not while typing in a box, nor with ⌘, Ctrl or Alt
  // (shortcuts).
  function typing(e: KeyboardEvent) {
    const t = e.target as HTMLElement | null;
    return e.metaKey || e.ctrlKey || e.altKey || !!t?.closest("input:not([type=checkbox]):not([type=range]), textarea, dialog");
  }

  function keyDown(e: KeyboardEvent) {
    if (typing(e)) return;
    const step = KEYS[e.code];
    if (step !== undefined) {
      e.preventDefault();
      if (e.repeat || byKey.has(e.code)) return;
      const note = base + step;
      if (note > 127) return;
      byKey.set(e.code, note);
      press(note);
    } else if (e.code === "KeyZ" || e.code === "KeyX") {
      e.preventDefault();
      base = Math.max(12, Math.min(120, base + (e.code === "KeyZ" ? -12 : 12)));
      showBase();
    } else if (e.code === "KeyC" || e.code === "KeyV") {
      e.preventDefault();
      setVelocity(velocity + (e.code === "KeyC" ? -10 : 10));
    }
  }

  function keyUp(e: KeyboardEvent) {
    const note = byKey.get(e.code);
    if (note === undefined) return;
    byKey.delete(e.code);
    lift(note);
  }

  // Keeps the octave the letters play in view.
  let scroller: HTMLElement | undefined = $state();
  function showBase() {
    requestAnimationFrame(() => {
      const el = scroller?.querySelector<HTMLElement>(`[data-note="${Math.max(LOW, base)}"]`);
      el?.scrollIntoView({ block: "nearest", inline: "nearest" });
    });
  }

  // The wheels: pitch bend springs back to the middle when let go, the
  // modulation wheel stays put. Drag, or ↑ ↓ (Shift for big steps).
  function bend(v: number) {
    pitch = Math.max(-8192, Math.min(8191, Math.round(v)));
    const raw = pitch + 8192;
    send([0xe0 | channel, raw & 0x7f, raw >> 7]);
  }
  function modulate(v: number) {
    const next = Math.max(0, Math.min(127, Math.round(v)));
    if (next === mod) return;
    mod = next;
    send([0xb0 | channel, 1, mod]);
  }
  function wheel(set: (f: number) => void, spring: boolean) {
    // f: 0 at the bottom of the wheel to 1 at the top.
    return {
      down(e: PointerEvent) {
        if (e.button !== 0) return;
        const el = e.currentTarget as HTMLElement;
        el.setPointerCapture(e.pointerId);
        el.focus();
        this.move(e);
      },
      move(e: PointerEvent) {
        const el = e.currentTarget as HTMLElement;
        if (!el.hasPointerCapture(e.pointerId)) return;
        const r = el.getBoundingClientRect();
        set(1 - (e.clientY - r.top) / r.height);
      },
      up() {
        if (spring) set(0.5);
      },
    };
  }
  const pitchWheel = wheel((f) => bend((f - 0.5) * 2 * 8192), true);
  const modWheel = wheel((f) => modulate(f * 127), false);
  function pitchKey(e: KeyboardEvent) {
    const by = e.shiftKey ? 4096 : 1024;
    if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      e.preventDefault();
      e.stopPropagation();
      bend(pitch + (e.key === "ArrowUp" ? by : -by));
    }
  }
  function pitchKeyUp(e: KeyboardEvent) {
    if (e.key === "ArrowUp" || e.key === "ArrowDown") bend(0);
  }
  function modKey(e: KeyboardEvent) {
    const by = e.shiftKey ? 16 : 1;
    const to: Record<string, number> = { ArrowUp: mod + by, ArrowDown: mod - by, Home: 0, End: 127 };
    if (e.key in to) {
      e.preventDefault();
      e.stopPropagation();
      modulate(to[e.key]);
    }
  }

  onMount(() => {
    void refresh();
    showBase();
  });
  onDestroy(silence);

  const missing = $derived(!!port && ports !== null && !ports.includes(port));
  const inRange = (n: number) => n >= base && n <= base + 17;
</script>

<svelte:window onkeydown={keyDown} onkeyup={keyUp} onblur={silence} onfocus={refresh} />

<div class="kbd">
  <div class="tools">
    <label>
      MIDI output
      <select value={port} onchange={(e) => setPort(e.currentTarget.value)} onfocus={refresh}>
        <option value="">{ports?.length === 0 ? "No MIDI outputs" : "Choose…"}</option>
        {#each ports ?? [] as p}<option value={p}>{p}</option>{/each}
        {#if missing}<option value={port}>{port} (not found)</option>{/if}
      </select>
    </label>
    <label>
      Channel
      <select value={String(channel)} onchange={(e) => setChannel(Number(e.currentTarget.value))}>
        {#each Array(16) as _, i}<option value={String(i)}>{i + 1}</option>{/each}
      </select>
    </label>
    <label>
      Velocity
      <input
        type="range"
        min="1"
        max="127"
        value={velocity}
        oninput={(e) => setVelocity(Number(e.currentTarget.value))}
        disabled={byHeight}
      />
      <span class="num">{byHeight ? "—" : velocity}</span>
    </label>
    <label title="Lower on the key plays louder">
      <input type="checkbox" checked={byHeight} onchange={(e) => app.save({ midiVelocityByHeight: e.currentTarget.checked })} />
      Velocity from where you click
    </label>
    <label>
      <input type="checkbox" checked={sustain} onchange={toggleSustain} disabled={!port} /> Sustain
    </label>
    <span class="spacer"></span>
    <button onclick={silence} title="Note off for every note, pedal up, pitch bend back to the middle">All notes off</button>
  </div>

  {#if !port || missing}
    <p class="hint">
      {#if ports?.length === 0}
        No MIDI outputs on this computer. Connect a MIDI interface whose output is cabled to the S5000's MIDI In.
      {:else if missing}
        “{port}” is not connected. Plug it in or choose another output.
      {:else}
        Choose the MIDI output cabled to the S5000's MIDI In. The keys light up without one, but nothing plays.
      {/if}
    </p>
  {/if}

  <div class="play">
    <div class="wheels">
      <div class="wheel-box">
        <div
          class="wheel"
          role="slider"
          tabindex="0"
          aria-label="Pitch bend"
          aria-valuemin={-8192}
          aria-valuemax={8191}
          aria-valuenow={pitch}
          onpointerdown={(e) => pitchWheel.down(e)}
          onpointermove={(e) => pitchWheel.move(e)}
          onpointerup={() => pitchWheel.up()}
          onpointercancel={() => pitchWheel.up()}
          onkeydown={pitchKey}
          onkeyup={pitchKeyUp}
        >
          <span class="mid"></span>
          <span class="thumb" style:bottom="{((pitch + 8192) / 16383) * 100}%"></span>
        </div>
        <span class="wheel-name">Pitch</span>
      </div>
      <div class="wheel-box">
        <div
          class="wheel"
          role="slider"
          tabindex="0"
          aria-label="Modulation"
          aria-valuemin={0}
          aria-valuemax={127}
          aria-valuenow={mod}
          onpointerdown={(e) => modWheel.down(e)}
          onpointermove={(e) => modWheel.move(e)}
          onpointerup={() => modWheel.up()}
          onpointercancel={() => modWheel.up()}
          onkeydown={modKey}
        >
          <span class="fill" style:height="{(mod / 127) * 100}%"></span>
          <span class="thumb" style:bottom="{(mod / 127) * 100}%"></span>
        </div>
        <span class="wheel-name">Mod</span>
      </div>
    </div>

    <div class="scroller" bind:this={scroller}>
      <div
        class="keys"
        style:--w={W}
        role="group"
        aria-label="Keyboard"
        onpointerdown={pointerDown}
        onpointermove={pointerMove}
        onpointerup={pointerUp}
        onpointercancel={pointerUp}
      >
        {#each whites as n}
          <div class="white" class:down={down.has(n)} data-note={n} title={noteName(n)}>
            {#if inRange(n)}<span class="letter">{LETTERS[n - base]}</span>{/if}
            {#if n % 12 === 0}<span class="name">{noteName(n)}</span>{/if}
          </div>
        {/each}
        {#each blacks as n}
          <div
            class="black"
            class:down={down.has(n)}
            data-note={n}
            title={noteName(n)}
            style:left="{blackLeft(n)}%"
            style:width="{(0.6 / W) * 100}%"
          >
            {#if inRange(n)}<span class="letter">{LETTERS[n - base]}</span>{/if}
          </div>
        {/each}
        <span
          class="reach"
          style:left="{(whites.filter((m) => m < base).length / W) * 100}%"
          style:width="{(whites.filter((m) => m >= base && m <= base + 17).length / W) * 100}%"
        ></span>
      </div>
    </div>
  </div>

  <p class="foot">
    Computer keys play from {noteName(base)}: A S D F G H J K L ; ' the white keys, W E T Y U O P the black ones.
    Z X octave down and up, C V velocity down and up.
  </p>
</div>

<style>
  .kbd {
    display: flex;
    flex-direction: column;
    gap: 12px;
    height: 100%;
    padding: 12px 16px;
    box-sizing: border-box;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 16px;
    flex-wrap: wrap;
    font-size: 12px;
  }
  .tools label {
    display: flex;
    align-items: center;
    gap: 6px;
    font-weight: 700;
  }
  .spacer {
    flex: 1;
  }
  input[type="range"] {
    -webkit-appearance: none;
    appearance: none;
    width: 130px;
    height: 16px;
    background: transparent;
  }
  input[type="range"]::-webkit-slider-runnable-track {
    height: 4px;
    background: var(--rule-soft);
  }
  input[type="range"]::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 8px;
    height: 16px;
    margin-top: -6px;
    background: var(--text);
  }
  input[type="range"]:disabled {
    opacity: 0.4;
  }
  input[type="range"]:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
  .num {
    width: 3ch;
    font-variant-numeric: tabular-nums;
  }
  .hint {
    margin: 0;
    font-size: 12px;
    color: var(--muted);
  }
  .play {
    flex: 1;
    min-height: 120px;
    max-height: 260px;
    display: flex;
    gap: 12px;
  }
  .wheels {
    display: flex;
    gap: 8px;
  }
  .wheel-box {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
  }
  .wheel {
    position: relative;
    flex: 1;
    width: 26px;
    background: var(--surface-2);
    border: 1px solid var(--rule);
    touch-action: none;
    cursor: ns-resize;
  }
  .wheel .mid {
    position: absolute;
    left: 0;
    right: 0;
    top: 50%;
    border-top: 1px dashed var(--rule-soft);
  }
  .wheel .fill {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    background: color-mix(in srgb, var(--accent) 35%, transparent);
  }
  .wheel .thumb {
    position: absolute;
    left: -1px;
    right: -1px;
    height: 6px;
    transform: translateY(50%);
    background: var(--text);
  }
  .wheel:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
  .wheel-name {
    font-size: 10px;
    font-weight: 700;
  }
  .scroller {
    flex: 1;
    min-width: 0;
    overflow-x: auto;
    overflow-y: hidden;
  }
  .keys {
    position: relative;
    display: flex;
    height: 100%;
    min-width: calc(var(--w) * 16px);
    padding-bottom: 8px;
    box-sizing: border-box;
    touch-action: none;
    user-select: none;
    -webkit-user-select: none;
  }
  .white {
    position: relative;
    flex: 1 1 0;
    background: #fff;
    border: 1px solid #000;
    border-left-width: 0;
    border-radius: 0 0 3px 3px;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    align-items: center;
    padding-bottom: 4px;
    color: #000;
    font-size: 9px;
  }
  .white:first-child {
    border-left-width: 1px;
  }
  .black {
    position: absolute;
    top: 0;
    height: 60%;
    background: #000;
    border-radius: 0 0 2px 2px;
    z-index: 1;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    align-items: center;
    padding-bottom: 3px;
    box-sizing: border-box;
    color: #fff;
    font-size: 9px;
  }
  .white.down {
    background: color-mix(in srgb, var(--accent) 60%, #fff);
  }
  .black.down {
    background: color-mix(in srgb, var(--accent) 70%, #000);
  }
  .letter {
    font-weight: 700;
    opacity: 0.6;
  }
  .name {
    color: #555;
  }
  /* The keys the letters reach, underlined. */
  .reach {
    position: absolute;
    bottom: 2px;
    height: 3px;
    background: var(--accent);
    pointer-events: none;
  }
  .foot {
    margin: 0;
    font-size: 12px;
    color: var(--muted);
  }
</style>
