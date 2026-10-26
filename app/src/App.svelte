<script lang="ts">
  import { onMount } from "svelte";
  import Dialog from "./lib/Dialog.svelte";
  import FrontPanel from "./lib/FrontPanel.svelte";
  import Meters from "./lib/Meters.svelte";
  import ProgramEditor from "./lib/ProgramEditor.svelte";
  import Transfers from "./lib/Transfers.svelte";
  import { queue } from "./lib/queue.svelte";
  import { app } from "./lib/state.svelte";
  import type { Mode } from "./lib/api";

  // One route bullet per view; ⌘1–⌘4 switch views.
  const views = [
    { id: "transfers", label: "Transfers", bullet: "blue" },
    { id: "panel", label: "Front panel", bullet: "red" },
    { id: "meters", label: "Meters", bullet: "green" },
    { id: "program", label: "Program", bullet: "orange" },
  ] as const;
  let view = $state<(typeof views)[number]["id"]>("transfers");
  let mode = $state<Mode>("simulator");
  let openProgram = $state<string | undefined>();

  onMount(async () => {
    await app.init();
    await queue.init();
    mode = app.settings.mode ?? "simulator";
    // For development: #connect=simulator&view=program&program=Drums connects
    // to the simulator and opens a view (and a program).
    const hash = new URLSearchParams(location.hash.slice(1));
    const v = views.find((x) => x.id === hash.get("view"));
    if (hash.get("connect") === "simulator") await app.connect("simulator");
    openProgram = hash.get("program") ?? undefined;
    if (v) view = v.id;
  });

  function keyDown(e: KeyboardEvent) {
    const n = parseInt(e.key);
    if (!e.metaKey || e.shiftKey || e.altKey || !(n >= 1 && n <= views.length)) return;
    e.preventDefault();
    view = views[n - 1].id;
  }
</script>

<svelte:window onkeydown={keyDown} />

<div class="shell">
  <header>
    <div class="brand">s5000-tools</div>
    <nav>
      {#each views as v, i}
        <button class:active={view === v.id} onclick={() => (view = v.id)} title="{v.label} (⌘{i + 1})">
          <span class="bullet {v.bullet}">{i + 1}</span>{v.label}
        </button>
      {/each}
    </nav>
    <div class="connection">
      {#if app.info}
        <span class="info" title="OS {app.info.os_version}">
          <b>{app.info.name}</b>
          <span>OS {app.info.os_version}</span>
          <span>Wave {app.info.wave_free}% free</span>
          <span>CPU {app.info.cpu_free}% free</span>
        </span>
        <button onclick={() => app.disconnect()} disabled={queue.running}>Disconnect</button>
      {:else}
        <select bind:value={mode} disabled={app.connecting} aria-label="Connection">
          <option value="simulator">Simulator</option>
          <option value="usb">Real sampler (USB)</option>
        </select>
        <button class="primary" onclick={() => app.connect(mode)} disabled={app.connecting}>
          {app.connecting ? "Connecting…" : "Connect"}
        </button>
      {/if}
    </div>
  </header>

  <main>
    {#if view === "transfers"}
      <Transfers />
    {:else if view === "panel"}
      <FrontPanel />
    {:else if view === "meters"}
      <Meters />
    {:else}
      <ProgramEditor initial={openProgram} />
    {/if}
  </main>

  <footer>
    <span class="dot" class:on={app.connected} class:sim={app.info?.simulated}></span>
    {#if app.error}
      <span class="error">{app.error}</span>
      <button class="link" onclick={() => (app.error = null)}>dismiss</button>
    {:else}
      <span>{app.connected && app.info?.simulated ? "Simulator · " : ""}{app.status}</span>
    {/if}
    <span class="spacer"></span>
    {#if queue.pending}<span>{queue.pending} transfer{queue.pending === 1 ? "" : "s"} pending</span>{/if}
  </footer>
</div>

<Dialog />

<style>
  .shell {
    display: grid;
    grid-template-rows: auto 1fr auto;
    height: 100vh;
  }
  /* The top bar is a sign: black, with a hairline white rule just below the top edge. */
  header {
    position: relative;
    display: flex;
    align-items: center;
    gap: 20px;
    padding: 10px 12px 7px;
    background: var(--sign-bg);
    color: var(--sign-text);
  }
  header::before {
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    top: 3px;
    border-top: 1px solid var(--sign-rule);
  }
  .brand {
    font-weight: 700;
    font-size: 15px;
    letter-spacing: -0.01em;
  }
  nav {
    display: flex;
    gap: 4px;
  }
  nav button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border: none;
    background: transparent;
    color: var(--sign-muted);
    padding: 3px 8px 3px 4px;
    font-size: 13px;
    border-bottom: 2px solid transparent;
  }
  nav button:hover:not(:disabled),
  nav button:active:not(:disabled) {
    background: transparent;
    color: var(--sign-text);
  }
  nav button.active {
    color: var(--sign-text);
    border-bottom-color: var(--sign-text);
  }
  .connection {
    margin-left: auto;
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .connection button,
  .connection select {
    background: transparent;
    color: var(--sign-text);
    border-color: var(--sign-muted);
  }
  .connection button:hover:not(:disabled) {
    background: transparent;
    border-color: var(--sign-text);
  }
  .connection button.primary {
    background: var(--sign-text);
    color: var(--sign-bg);
    border-color: var(--sign-text);
  }
  .connection select option {
    background: var(--surface);
    color: var(--text);
  }
  .info {
    display: flex;
    gap: 12px;
    font-size: 12px;
    color: var(--sign-muted);
    white-space: nowrap;
  }
  .info b {
    color: var(--sign-text);
  }
  main {
    min-height: 0;
    overflow: auto;
  }
  footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 12px;
    font-size: 12px;
    border-top: 1px solid var(--rule);
    background: var(--surface);
    color: var(--muted);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--rule-soft);
  }
  .dot.on {
    background: var(--ok);
  }
  .dot.sim {
    background: var(--mta-yellow);
  }
  .error {
    color: var(--bad);
    font-weight: 700;
    -webkit-user-select: text;
    user-select: text;
  }
  .link {
    border: none;
    background: none;
    color: var(--muted);
    font-weight: 400;
    text-decoration: underline;
    padding: 0;
  }
  .spacer {
    flex: 1;
  }
</style>
