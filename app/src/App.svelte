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

  const views = [
    { id: "transfers", label: "Transfers" },
    { id: "panel", label: "Front panel" },
    { id: "meters", label: "Meters" },
    { id: "program", label: "Program" },
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
</script>

<div class="shell">
  <header>
    <div class="brand">s5000-tools</div>
    <nav>
      {#each views as v}
        <button class:active={view === v.id} onclick={() => (view = v.id)}>{v.label}</button>
      {/each}
    </nav>
    <div class="connection">
      {#if app.info}
        <span class="lcd" title="OS {app.info.os_version}">
          {app.info.name} · OS {app.info.os_version} · wave {app.info.wave_free}% · CPU {app.info.cpu_free}% free
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
  header {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 8px 12px;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
  }
  .brand {
    font-weight: 700;
    letter-spacing: 0.02em;
  }
  nav {
    display: flex;
    gap: 2px;
    background: var(--panel-2);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 2px;
  }
  nav button {
    border: none;
    background: transparent;
    padding: 3px 12px;
  }
  nav button.active {
    background: var(--panel);
    box-shadow: 0 0 0 1px var(--line);
    font-weight: 600;
  }
  .connection {
    margin-left: auto;
    display: flex;
    gap: 8px;
    align-items: center;
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
    border-top: 1px solid var(--line);
    background: var(--panel);
    color: var(--muted);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--line);
  }
  .dot.on {
    background: var(--ok);
  }
  .dot.sim {
    background: var(--accent);
  }
  .error {
    color: var(--bad);
    -webkit-user-select: text;
    user-select: text;
  }
  .link {
    border: none;
    background: none;
    color: var(--muted);
    text-decoration: underline;
    padding: 0;
  }
  .spacer {
    flex: 1;
  }
</style>
