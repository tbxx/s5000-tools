<script lang="ts">
  // A keyboard window: play a sampler from the screen. Notes go to it over
  // USB, as MIDI channel messages (midi.rs); each connected sampler can have
  // a keyboard window of its own (the window's item is its short id).
  //
  // Click or drag across the keys (a glissando); with "Velocity from where
  // you click", the further down the key, the louder. The computer keyboard
  // plays too: A to ' are the white keys from the octave marked under the
  // keys, W E T Y U O P the black ones; Z and X change the octave, C and V
  // the velocity. Notes stop when the window loses focus or closes.
  //
  // The arpeggiator plays the held keys one at a time instead: up, down, up
  // and down, in the order played or at random, over one to four octaves, at
  // a tempo and note length. Latch keeps a chord going after the keys are let
  // go (and while the window is in the background); the next chord replaces
  // it. Its clock runs here, each step timed from the start so it does not
  // drift.
  import { onDestroy, onMount } from "svelte";
  import { SvelteMap } from "svelte/reactivity";
  import { api, type Settings } from "./api";
  import { noteName } from "./format";
  import { app } from "./state.svelte";
  import { here, windows } from "./windows.svelte";

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

  // The sampler this window plays: the one it was opened for, else the
  // active one, which it then keeps.
  const connected = $derived(app.samplers.filter((s) => s.status === "connected"));
  let unit = $state<string | undefined>(here.item);
  const target = $derived(
    unit === undefined
      ? app.active?.status === "connected"
        ? app.active
        : undefined
      : connected.find((s) => s.short_id === unit),
  );
  $effect(() => {
    if (unit === undefined && target) choose(target.short_id);
  });
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

  function send(msg: number[]) {
    if (!target) return;
    api.midiSend(target.id, msg).catch((e) => app.fail(e));
  }

  const on = (note: number, vel: number) => send([0x90 | channel, note, Math.max(1, Math.min(127, vel))]);
  const off = (note: number) => send([0x80 | channel, note, 64]);

  function press(note: number, vel = velocity) {
    if (note < 0 || note > 127) return;
    const n = down.get(note) ?? 0;
    down.set(note, n + 1);
    if (n > 0) return;
    if (arp.on) arpAdd(note, vel);
    else on(note, vel);
  }

  function lift(note: number | undefined) {
    if (note === undefined) return;
    const n = down.get(note) ?? 0;
    if (n <= 1) {
      down.delete(note);
      if (n !== 1) return;
      if (arp.on) arpRemove(note);
      else off(note);
    } else down.set(note, n - 1);
  }

  // --- The arpeggiator ---------------------------------------------------------

  type ArpMode = "up" | "down" | "updown" | "played" | "random";
  const MODES: [ArpMode, string][] = [
    ["up", "Up"],
    ["down", "Down"],
    ["updown", "Up and down"],
    ["played", "As played"],
    ["random", "Random"],
  ];
  /** Note lengths, in beats. */
  const RATES: [string, number][] = [
    ["1/4", 1],
    ["1/8", 1 / 2],
    ["1/8 T", 1 / 3],
    ["1/16", 1 / 4],
    ["1/16 T", 1 / 6],
    ["1/32", 1 / 8],
  ];
  const arp = $derived({
    on: app.settings.arpOn ?? false,
    mode: (app.settings.arpMode ?? "up") as ArpMode,
    rate: app.settings.arpRate ?? "1/16",
    tempo: app.settings.arpTempo ?? 120,
    octaves: app.settings.arpOctaves ?? 1,
    gate: app.settings.arpGate ?? 50,
    latch: app.settings.arpLatch ?? false,
  });
  /** The notes the arpeggiator plays, in the order pressed. */
  let chord = $state<{ note: number; vel: number }[]>([]);
  const inChord = $derived(new Set(chord.map((c) => c.note)));
  /** The note it is sounding now. */
  let sounding = $state<number | null>(null);
  let step = 0;
  let nextAt = 0;
  let clock: ReturnType<typeof setTimeout> | undefined;
  let gateTimer: ReturnType<typeof setTimeout> | undefined;

  const stepMs = () => (60000 / arp.tempo) * (RATES.find(([r]) => r === arp.rate)?.[1] ?? 1 / 4);

  function arpAdd(note: number, vel: number) {
    // With latch, the first key after all were let go starts a new chord.
    if (arp.latch && down.size === 1) chord = [];
    if (!inChord.has(note)) chord = [...chord, { note, vel }];
    if (clock === undefined) {
      step = 0;
      nextAt = performance.now();
      tick();
    }
  }

  function arpRemove(note: number) {
    if (arp.latch) return;
    chord = chord.filter((c) => c.note !== note);
    if (!chord.length) arpStop();
  }

  /** The notes in playing order. */
  function sequence() {
    const ordered = arp.mode === "played" ? chord : [...chord].sort((a, b) => a.note - b.note);
    let seq = [];
    for (let o = 0; o < arp.octaves; o++)
      for (const c of ordered) if (c.note + 12 * o <= 127) seq.push({ note: c.note + 12 * o, vel: c.vel });
    if (arp.mode === "down") seq.reverse();
    if (arp.mode === "updown") seq = [...seq, ...seq.slice(1, -1).reverse()];
    return seq;
  }

  function tick() {
    const seq = sequence();
    if (!seq.length) return arpStop();
    const next = arp.mode === "random" ? seq[Math.floor(Math.random() * seq.length)] : seq[step % seq.length];
    step++;
    if (sounding !== null) off(sounding);
    clearTimeout(gateTimer);
    on(next.note, next.vel);
    sounding = next.note;
    const len = stepMs();
    if (arp.gate < 100)
      gateTimer = setTimeout(() => {
        if (sounding === next.note) {
          off(next.note);
          sounding = null;
        }
      }, (len * arp.gate) / 100);
    nextAt += len;
    // After a stall (a sleeping laptop), carry on from now rather than catch up.
    if (nextAt < performance.now() - len) nextAt = performance.now() + len;
    clock = setTimeout(tick, Math.max(0, nextAt - performance.now()));
  }

  function arpStop() {
    clearTimeout(clock);
    clearTimeout(gateTimer);
    clock = gateTimer = undefined;
    if (sounding !== null) off(sounding);
    sounding = null;
  }

  function setArp(patch: Partial<Settings>) {
    // Switching it on or off, or latch off, starts afresh.
    if ("arpOn" in patch || patch.arpLatch === false) silence();
    app.save(patch);
  }

  /** Stops everything: every note, the pedal, the wheels back to rest. */
  function silence() {
    arpStop();
    chord = [];
    down.clear();
    byPointer.clear();
    byKey.clear();
    if (sustain) send([0xb0 | channel, 64, 0]);
    sustain = false;
    if (pitch !== 0) send([0xe0 | channel, 0, 64]);
    pitch = 0;
    void api.midiReleaseAll().catch(() => {});
  }

  function choose(shortId: string) {
    if (unit !== undefined) silence();
    unit = shortId;
    void windows.showing(shortId);
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

  /** In the background a key's release may never arrive: let go of the keys.
   *  A latched arpeggio plays on. */
  function blur() {
    if (!(arp.on && arp.latch && chord.length)) return silence();
    down.clear();
    byPointer.clear();
    byKey.clear();
  }

  onMount(showBase);
  onDestroy(silence);

  const inRange = (n: number) => n >= base && n <= base + 17;
</script>

<svelte:window onkeydown={keyDown} onkeyup={keyUp} onblur={blur} />

<div class="kbd">
  <div class="tools">
    <label>
      Sampler
      <select value={unit ?? ""} onchange={(e) => choose(e.currentTarget.value)} disabled={!connected.length}>
        {#if !target}<option value={unit ?? ""}>{unit ? `${unit} (not connected)` : "Not connected"}</option>{/if}
        {#each connected as s}<option value={s.short_id}>{s.product} · {s.short_id}</option>{/each}
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
      <input type="checkbox" checked={sustain} onchange={toggleSustain} disabled={!target} /> Sustain
    </label>
    <span class="spacer"></span>
    <button onclick={silence} title="Note off for every note, pedal up, pitch bend back to the middle">All notes off</button>
  </div>

  <div class="tools arp" class:off={!arp.on}>
    <label><input type="checkbox" checked={arp.on} onchange={(e) => setArp({ arpOn: e.currentTarget.checked })} /> Arpeggiator</label>
    <label>
      Mode
      <select value={arp.mode} onchange={(e) => setArp({ arpMode: e.currentTarget.value as ArpMode })}>
        {#each MODES as [id, name]}<option value={id}>{name}</option>{/each}
      </select>
    </label>
    <label>
      Rate
      <select value={arp.rate} onchange={(e) => setArp({ arpRate: e.currentTarget.value })}>
        {#each RATES as [r]}<option value={r}>{r}</option>{/each}
      </select>
    </label>
    <label>
      Tempo
      <input
        class="tempo"
        type="number"
        min="30"
        max="300"
        value={arp.tempo}
        onchange={(e) => {
          const v = Math.round(Number(e.currentTarget.value));
          if (v >= 30 && v <= 300) setArp({ arpTempo: v });
          else e.currentTarget.value = String(arp.tempo);
        }}
      />
      BPM
    </label>
    <label>
      Octaves
      <select value={String(arp.octaves)} onchange={(e) => setArp({ arpOctaves: Number(e.currentTarget.value) })}>
        {#each [1, 2, 3, 4] as o}<option value={String(o)}>{o}</option>{/each}
      </select>
    </label>
    <label>
      Length
      <select value={String(arp.gate)} onchange={(e) => setArp({ arpGate: Number(e.currentTarget.value) })}>
        {#each [10, 25, 50, 75, 90, 100] as g}<option value={String(g)}>{g}%</option>{/each}
      </select>
    </label>
    <label title="Keep playing after the keys are let go; the next chord replaces it">
      <input type="checkbox" checked={arp.latch} onchange={(e) => setArp({ arpLatch: e.currentTarget.checked })} /> Latch
    </label>
  </div>

  {#if !target}
    <p class="hint">
      {unit ? `${unit} is not connected.` : "No sampler is connected."} The keys light up, but nothing plays until one
      is.
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
          <div
            class="white"
            class:down={down.has(n) || (arp.on && inChord.has(n))}
            class:playing={sounding === n}
            data-note={n}
            title={noteName(n)}
          >
            {#if inRange(n)}<span class="letter">{LETTERS[n - base]}</span>{/if}
            {#if n % 12 === 0}<span class="name">{noteName(n)}</span>{/if}
          </div>
        {/each}
        {#each blacks as n}
          <div
            class="black"
            class:down={down.has(n) || (arp.on && inChord.has(n))}
            class:playing={sounding === n}
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
  /* Piano proportions: a white key six times as long as it is wide, black
     keys 0.6 as wide and two-thirds as long. */
  .kbd {
    --kw: 24px;
    --kh: calc(var(--kw) * 6);
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
    flex: none;
    height: calc(var(--kh) + 10px);
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
    width: calc(var(--w) * var(--kw));
    height: calc(var(--kh) + 8px);
    padding-bottom: 8px;
    box-sizing: border-box;
    touch-action: none;
    user-select: none;
    -webkit-user-select: none;
  }
  .white {
    position: relative;
    flex: none;
    width: var(--kw);
    box-sizing: border-box;
    background: linear-gradient(to bottom, #e9e9e9, #fff 12%, #fff 92%, #ececec);
    border: 1px solid #333;
    border-left-width: 0;
    border-radius: 0 0 4px 4px;
    box-shadow: inset 0 -4px 0 #d9d9d9;
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
    height: calc(var(--kh) * 0.64);
    background: linear-gradient(to bottom, #000, #2b2b2b 85%, #3a3a3a);
    border-radius: 0 0 3px 3px;
    box-shadow:
      inset 0 -6px 0 #111,
      1px 2px 3px rgb(0 0 0 / 0.35);
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
  /* Pressed: lit, and the shading of the front edge gone, as if pushed down. */
  .white.down {
    background: color-mix(in srgb, var(--accent) 55%, #fff);
    box-shadow: inset 0 2px 4px rgb(0 0 0 / 0.25);
  }
  .black.down {
    background: color-mix(in srgb, var(--accent) 65%, #000);
    box-shadow: inset 0 -2px 0 #111;
  }
  .white.playing {
    background: var(--accent);
  }
  .black.playing {
    background: var(--accent);
  }
  .arp.off label:not(:first-child) {
    opacity: 0.55;
  }
  .tempo {
    width: 64px;
    box-sizing: border-box;
    font-variant-numeric: tabular-nums;
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
