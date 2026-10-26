<script lang="ts">
  import { onMount } from "svelte";
  import DataWheel from "./DataWheel.svelte";
  import { api, poll } from "./api";
  import { app } from "./state.svelte";

  const W = 320;
  const H = 240;

  // Key codes: s5k::Key.
  const modeKeys = [
    { code: 0x40, label: "FX" },
    { code: 0x41, label: "Record" },
    { code: 0x42, label: "Edit Sample" },
    { code: 0x43, label: "Edit Program" },
    { code: 0x44, label: "Multi" },
    { code: 0x45, label: "Utilities" },
    { code: 0x46, label: "Save" },
    { code: 0x47, label: "Load" },
  ];
  const fn = (n: number) => ({ code: 0x47 + n, label: `F${n}` });
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
      draw(await api.screen());
      live = true;
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
    onpointerdown={(e) => {
      e.currentTarget.setPointerCapture(e.pointerId);
      press(k.code);
    }}
    onpointerup={() => release(k.code)}
    onpointercancel={() => release(k.code)}
    onlostpointercapture={() => release(k.code)}>{k.label}</button
  >
{/snippet}

<div class="panel">
  <div class="left">
    <div class="screen" class:off={!live}>
      <canvas bind:this={canvas} width={W} height={H}></canvas>
      {#if !live}<div class="overlay">{app.connected ? "Waiting for the screen…" : "Not connected"}</div>{/if}
    </div>
    <div class="soft">
      {#each [1, 2, 3, 4, 5, 6, 7, 8] as n}{@render key(fn(n), "f")}{/each}
    </div>
    <div class="soft">
      {#each [9, 10, 11, 12, 13, 14, 15, 16] as n}{@render key(fn(n), "f")}{/each}
    </div>
    <p class="hint muted">
      Keyboard: ← → cursor, 0–9, + and −, Enter = Ent/Play, Esc = Exit. Scroll or drag the wheel.
    </p>
  </div>

  <div class="right">
    <div class="group">
      <span class="title">Mode</span>
      <div class="modes">
        {#each modeKeys as k, i}
          <div class="mode">
            <span class="led" class:lit={(leds >> i) & 1}></span>
            {@render key(k)}
          </div>
        {/each}
      </div>
    </div>

    <div class="group wheel-row">
      <div class="cursor">
        <span class="title">Cursor</span>
        <div class="pair">{@render key(left)}{@render key(right)}</div>
        <span class="title">Data</span>
        <div class="pair">{@render key(minus)}{@render key(plus)}</div>
      </div>
      <DataWheel onturn={(n) => app.connected && api.wheel(n).catch((e) => app.fail(e))} />
    </div>

    <div class="group pads">
      <div>
        <span class="title">Numbers</span>
        <div class="numpad">
          {#each numKeys as row}{#each row as k}{@render key(k)}{/each}{/each}
          {@render key(minus)}{@render key(zero)}{@render key(plus)}
        </div>
      </div>
      <div>
        <span class="title">Edit</span>
        <div class="edit">
          {#each editKeys as k}{@render key(k)}{/each}
          {@render key(entPlay, "ent")}
        </div>
      </div>
    </div>

    <div class="group">
      <span class="title">User</span>
      <div class="users">{#each userKeys as k}{@render key(k)}{/each}</div>
    </div>
  </div>
</div>

<style>
  .panel {
    display: flex;
    gap: 24px;
    padding: 18px;
    justify-content: center;
    align-items: flex-start;
    min-width: fit-content;
  }
  .left {
    width: 664px;
  }
  .screen {
    position: relative;
    padding: 12px;
    background: var(--bezel);
    border-radius: 6px;
  }
  canvas {
    display: block;
    width: 640px;
    height: 480px;
    image-rendering: pixelated;
    background: var(--lcd-bg);
  }
  .off canvas {
    opacity: 0.5;
  }
  .overlay {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: var(--lcd-fg);
    font-family: var(--mono);
  }
  .soft {
    display: grid;
    grid-template-columns: repeat(8, 1fr);
    gap: 8px;
    padding: 10px 12px 0;
  }
  .hint {
    font-size: 11px;
    margin: 12px;
  }
  .right {
    display: flex;
    flex-direction: column;
    gap: 14px;
    width: 300px;
  }
  .group {
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 8px 10px 10px;
  }
  .group > .title,
  .group > div > .title,
  .cursor .title {
    margin-bottom: 6px;
  }
  .modes {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 6px 10px;
  }
  .mode {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .mode .key {
    flex: 1;
  }
  .led {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--track);
    flex: none;
  }
  .led.lit {
    background: #ff5a2a;
    box-shadow: 0 0 6px #ff5a2a;
  }
  .wheel-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .cursor {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .pair {
    display: flex;
    gap: 6px;
    margin-bottom: 6px;
  }
  .pair .key {
    width: 48px;
  }
  .pads {
    display: flex;
    gap: 14px;
  }
  .numpad {
    display: grid;
    grid-template-columns: repeat(3, 40px);
    gap: 5px;
  }
  .edit {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 5px;
  }
  .ent {
    grid-column: span 2;
  }
  .users {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 6px;
  }
  .key {
    background: var(--key);
    border: 1px solid var(--key-edge);
    border-bottom-width: 3px;
    border-radius: 4px;
    padding: 5px 4px;
    font-size: 11px;
    font-weight: 600;
    white-space: nowrap;
    touch-action: none;
  }
  .key.f {
    padding: 8px 0;
  }
  .key.down {
    border-bottom-width: 1px;
    transform: translateY(2px);
    background: var(--panel-2);
  }
</style>
