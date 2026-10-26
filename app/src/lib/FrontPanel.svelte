<script lang="ts">
  import { capture } from "./pointer";
  import { onMount } from "svelte";
  import DataWheel from "./DataWheel.svelte";
  import { api, poll } from "./api";
  import { app } from "./state.svelte";

  const W = 320;
  const H = 240;
  /** The LCD has 8 rows of 30 px; F1–F8 sit left of rows 0–7, F9–F16 right. */
  const ROWS = 8;
  const rows = [...Array(ROWS).keys()];
  const SCALES = [1, 1.5, 2];

  // Key codes: s5k::Key. In the sampler's left-to-right order; a key's LED is
  // bit (code - 0x40) of the LED byte.
  const modeKeys = [
    { code: 0x44, label: "Multi" },
    { code: 0x40, label: "FX" },
    { code: 0x42, label: "Edit Sample" },
    { code: 0x43, label: "Edit Program" },
    { code: 0x41, label: "Record" },
    { code: 0x45, label: "Utilities" },
    { code: 0x46, label: "Save" },
    { code: 0x47, label: "Load" },
  ];
  const fn = (n: number) => ({ code: 0x47 + n, label: `F${n}` });
  /** The soft key for row `row` on the left or right of the screen. */
  const softKey = (row: number, right: boolean) => fn(row + 1 + (right ? ROWS : 0));
  const numKeys = [
    [7, 8, 9],
    [4, 5, 6],
    [1, 2, 3],
  ].map((row) => row.map((n) => ({ code: 0x58 + n, label: String(n) })));
  const minus = { code: 0x62, label: "−" };
  const zero = { code: 0x58, label: "0" };
  const plus = { code: 0x63, label: "+" };
  const left = { code: 0x64, label: "◀" };
  const right = { code: 0x65, label: "▶" };
  const editKeys = [
    { code: 0x67, label: "Window" },
    { code: 0x68, label: "Mark" },
    { code: 0x69, label: "Jump" },
    { code: 0x6a, label: "Exit" },
  ];
  const entPlay = { code: 0x6b, label: "Ent / Play" };
  const userKeys = [0x6c, 0x6d, 0x6e].map((code, i) => ({ code, label: `User ${i + 1}` }));

  /** Computer keys mapped to panel keys. */
  const shortcuts: Record<string, number> = {
    ArrowLeft: 0x64,
    ArrowRight: 0x65,
    Enter: 0x6b,
    Escape: 0x6a,
    "-": 0x62,
    "+": 0x63,
    "=": 0x63,
    ...Object.fromEntries([...Array(10).keys()].map((n) => [String(n), 0x58 + n])),
  };

  let canvas: HTMLCanvasElement;
  let leds = $state(0);
  let held = $state(new Set<number>());
  let live = $state(false);
  let scale = $state(loadScale());

  function loadScale() {
    try {
      const v = parseFloat(localStorage.getItem("panel.scale") ?? "");
      return SCALES.includes(v) ? v : 2;
    } catch {
      return 2;
    }
  }

  function setScale(v: number) {
    scale = v;
    try {
      localStorage.setItem("panel.scale", String(v));
    } catch {
      // Not remembered; fine.
    }
  }
  function colour(name: string): [number, number, number] {
    const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
    const n = parseInt(v.slice(1), 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
  }

  function draw(buf: ArrayBuffer) {
    const px = new Uint8Array(buf);
    if (px.length < W * H + 1) return;
    const ctx = canvas.getContext("2d")!;
    const img = ctx.createImageData(W, H);
    const [bg, fg] = [colour("--lcd-bg"), colour("--lcd-fg")];
    for (let i = 0; i < W * H; i++) {
      const c = px[i] ? fg : bg;
      img.data.set(c, i * 4);
      img.data[i * 4 + 3] = 255;
    }
    ctx.putImageData(img, 0, 0);
    leds = px[W * H];
  }

  async function press(code: number) {
    if (held.has(code) || !app.connected) return;
    held = new Set(held).add(code);
    await api.press(code).catch((e) => app.fail(e));
  }

  async function release(code: number) {
    if (!held.has(code)) return;
    const next = new Set(held);
    next.delete(code);
    held = next;
    await api.release(code).catch((e) => app.fail(e));
  }

  // A mouse holds a key from pointerdown to pointerup. Keyboard focus (Space,
  // Enter), VoiceOver and other assistive clicks fire only `click`: tap then.
  let pointerPressed = false;

  function down(e: PointerEvent, code: number) {
    pointerPressed = true;
    press(code);
    capture(e);
  }

  async function clicked(code: number) {
    if (pointerPressed) {
      pointerPressed = false;
      return;
    }
    await press(code);
    await release(code);
  }

  function keyDown(e: KeyboardEvent) {
    const code = shortcuts[e.key];
    if (code === undefined || e.repeat || e.metaKey || e.ctrlKey) return;
    if ((e.target as HTMLElement).closest("input, select, textarea")) return;
    e.preventDefault();
    press(code);
  }

  function keyUp(e: KeyboardEvent) {
    const code = shortcuts[e.key];
    if (code !== undefined) release(code);
  }

  onMount(() => {
    const stop = poll(150, async () => {
      if (!app.connected) {
        live = false;
        return;
      }
      try {
        draw(await api.screen());
        live = true;
      } catch {
        // Dim the screen rather than show a stale image as if it were live.
        live = false;
      }
    });
    return () => {
      stop();
      for (const code of held) api.release(code).catch(() => {});
    };
  });
</script>

<svelte:window onkeydown={keyDown} onkeyup={keyUp} />

{#snippet key(k: { code: number; label: string }, cls = "")}
  <button
    class="key {cls}"
    class:down={held.has(k.code)}
    disabled={!app.connected}
    onpointerdown={(e) => down(e, k.code)}
    onpointerup={() => release(k.code)}
    onclick={() => clicked(k.code)}
    onpointercancel={() => release(k.code)}
    onlostpointercapture={() => release(k.code)}>{k.label}</button
  >
{/snippet}

{#snippet softColumn(right: boolean)}
  <div class="fcol" class:right>
    {#each rows as r}{@render key(softKey(r, right), "f")}{/each}
  </div>
{/snippet}

<div class="panel" style:--s={scale}>
  <div class="main">
    <div class="display">
      {@render softColumn(false)}
      <div class="screen" class:off={!live}>
        <canvas bind:this={canvas} width={W} height={H}></canvas>
        <!-- Clicking the left half of row k presses F(k+1), the right half F(9+k). -->
        <div class="hot" class:active={live}>
          {#each rows as r}
            {#each [false, true] as right}
              {@const k = softKey(r, right)}
              <button
                class="zone"
                class:down={held.has(k.code)}
                tabindex="-1"
                aria-label={k.label}
                title={k.label}
                disabled={!app.connected}
                onpointerdown={(e) => down(e, k.code)}
                onpointerup={() => release(k.code)}
                onclick={() => clicked(k.code)}
                onpointercancel={() => release(k.code)}
                onlostpointercapture={() => release(k.code)}
              ></button>
            {/each}
          {/each}
        </div>
        {#if !live}<div class="overlay">{app.connected ? "Waiting for the screen…" : "Not connected"}</div>{/if}
      </div>
      {@render softColumn(true)}
    </div>

    <div class="modes">
      {#each modeKeys as k}
        <div class="mode">
          <span class="led" class:lit={(leds >> (k.code - 0x40)) & 1}></span>
          {@render key(k)}
        </div>
      {/each}
    </div>

    <div class="foot">
      <p class="hint muted">
        Click a row of the screen to press its soft key. Keyboard: ← → cursor, 0–9, + and −, Enter = Ent/Play, Esc =
        Exit. Scroll or drag the wheel.
      </p>
      <div class="scale" role="group" aria-label="Screen size">
        {#each SCALES as v}
          <button class:active={scale === v} onclick={() => setScale(v)}>{v}×</button>
        {/each}
      </div>
    </div>
  </div>

  <div class="side">
    <section>
      <h3 class="sign">Cursor and data</h3>
      <div class="wheel-row">
        <div class="cursor">
          <div class="pair">{@render key(left)}{@render key(right)}</div>
          <div class="pair">{@render key(minus)}{@render key(plus)}</div>
        </div>
        <DataWheel onturn={(n) => app.connected && api.wheel(n).catch((e) => app.fail(e))} />
      </div>
    </section>

    <section class="pads">
      <div>
        <h3 class="sign">Numbers</h3>
        <div class="numpad">
          {#each numKeys as row}{#each row as k}{@render key(k)}{/each}{/each}
          {@render key(minus)}{@render key(zero)}{@render key(plus)}
        </div>
      </div>
      <div class="edit-col">
        <h3 class="sign">Edit</h3>
        <div class="edit">
          {#each editKeys as k}{@render key(k)}{/each}
          {@render key(entPlay, "ent")}
        </div>
      </div>
    </section>

    <section>
      <h3 class="sign">User</h3>
      <div class="users">{#each userKeys as k}{@render key(k)}{/each}</div>
    </section>
  </div>
</div>

<style>
  .panel {
    --s: 2;
    --row: calc(30px * var(--s));
    display: flex;
    gap: 24px;
    padding: 18px;
    justify-content: center;
    align-items: flex-start;
    min-width: fit-content;
  }
  .main {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  /* The display is a sign: black, with the screen between two columns of soft keys. */
  .display {
    display: grid;
    grid-template-columns: auto auto auto;
    justify-content: center;
    gap: 10px;
    padding: 14px 12px;
    background: var(--sign-bg);
  }
  /* Each column is exactly as tall as the screen, one cell per 30 px LCD row,
     so a key sits centred on its row at any scale. */
  .fcol {
    display: grid;
    grid-template-rows: repeat(8, var(--row));
    align-items: center;
  }
  .screen {
    position: relative;
    width: calc(320px * var(--s));
    height: calc(240px * var(--s));
    outline: 1px solid var(--lcd-edge);
  }
  canvas {
    display: block;
    width: 100%;
    height: 100%;
    image-rendering: pixelated;
    background: var(--lcd-bg);
  }
  .off canvas {
    opacity: 0.4;
  }
  .overlay {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: var(--lcd-fg);
    font-weight: 700;
    pointer-events: none;
  }
  .hot {
    position: absolute;
    inset: 0;
    display: grid;
    grid-template-columns: 1fr 1fr;
    grid-template-rows: repeat(8, 1fr);
  }
  /* No outline or tint: the zones are a plain 2 x 8 grid and don't match the
     shapes the sampler draws. The F-key beside the row shows the press. */
  .hot .zone,
  .hot .zone:hover,
  .hot .zone:active,
  .hot .zone:focus,
  .hot .zone.down {
    border: none;
    padding: 0;
    background: transparent;
    box-shadow: none;
    outline: none;
    touch-action: none;
  }
  .hot.active .zone:not(:disabled) {
    cursor: pointer;
  }

  .key {
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--rule);
    padding: 5px 4px;
    font-size: 11px;
    font-weight: 700;
    white-space: nowrap;
    touch-action: none;
  }
  .key.down,
  .key:active:not(:disabled) {
    background: var(--text);
    color: var(--bg);
  }
  .key.f {
    width: 46px;
    height: min(26px, calc(var(--row) - 6px));
    padding: 0;
    background: transparent;
    color: var(--sign-text);
    border-color: var(--sign-muted);
  }
  .key.f:hover:not(:disabled) {
    border-color: var(--sign-text);
  }
  .key.f.down,
  .key.f:active:not(:disabled) {
    background: var(--sign-text);
    color: var(--sign-bg);
  }

  .modes {
    display: grid;
    grid-template-columns: repeat(8, 1fr);
    gap: 8px;
  }
  .mode {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 5px;
  }
  .mode .key {
    width: 100%;
    padding: 7px 2px;
    white-space: normal;
    line-height: 1.1;
    min-height: 38px;
  }
  .led {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    border: 1px solid var(--rule-soft);
    background: transparent;
  }
  .led.lit {
    border-color: var(--led);
    background: var(--led);
  }
  .foot {
    display: flex;
    align-items: flex-start;
    gap: 16px;
  }
  .hint {
    flex: 1;
    font-size: 11px;
    margin: 0;
    max-width: 520px;
  }
  .scale {
    display: flex;
  }
  .scale button {
    font-size: 11px;
    padding: 1px 7px;
    margin-left: -1px;
  }
  .scale button.active {
    background: var(--text);
    color: var(--bg);
  }

  .side {
    display: flex;
    flex-direction: column;
    gap: 18px;
    width: 268px;
  }
  .side .sign {
    margin: 0 0 8px;
  }
  .wheel-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .cursor {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .pair {
    display: flex;
    gap: 6px;
  }
  .pair .key {
    width: 48px;
    padding: 7px 0;
  }
  .pads {
    display: flex;
    gap: 14px;
  }
  .edit-col {
    flex: 1;
  }
  .numpad {
    display: grid;
    grid-template-columns: repeat(3, 38px);
    gap: 5px;
  }
  .numpad .key {
    padding: 7px 0;
  }
  .edit {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 5px;
  }
  .edit .key {
    padding: 7px 2px;
  }
  .ent {
    grid-column: span 2;
  }
  .users {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 6px;
  }
</style>
