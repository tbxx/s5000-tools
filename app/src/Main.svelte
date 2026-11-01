<script lang="ts">
  import { onMount } from "svelte";
  import Dialog from "./lib/Dialog.svelte";
  import FrontPanel from "./lib/FrontPanel.svelte";
  import Meters from "./lib/Meters.svelte";
  import MultiEditor from "./lib/MultiEditor.svelte";
  import Preferences from "./lib/Preferences.svelte";
  import ProgramEditor from "./lib/ProgramEditor.svelte";
  import Transfers from "./lib/Transfers.svelte";
  import { queue } from "./lib/queue.svelte";
  import { app } from "./lib/state.svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api, usbLabel, type UsbDevice } from "./lib/api";
  import { initCommands, onKey } from "./lib/commands";
  import { autoConnect, connectAll, modLabel } from "./lib/prefs.svelte";
  import { windows, type WindowKind } from "./lib/windows.svelte";

  // One route bullet per view; ⌘1–⌘5 (Ctrl elsewhere) switch views.
  const views = [
    { id: "transfers", label: "Transfers", bullet: "blue" },
    { id: "panel", label: "Front panel", bullet: "red" },
    { id: "meters", label: "Meters", bullet: "green" },
    { id: "program", label: "Program", bullet: "orange" },
    { id: "multi", label: "Multi", bullet: "purple" },
  ] as const;
  type View = (typeof views)[number]["id"];
  let view = $state<View>("transfers");
  // The Connect menu: each USB sampler not yet connected, then "simulator".
  // A hardware sampler is preselected whenever one is visible, unless the
  // user has picked something else.
  let usb = $state<UsbDevice[]>([]);
  let choice = $state("simulator");
  let picked = false;

  async function scan() {
    try {
      usb = (await api.usbDevices()).filter((d) => d.connected === null);
    } catch {
      usb = [];
    }
    const keys = usb.map((d) => d.key);
    if (!picked || (choice !== "simulator" && !keys.includes(choice))) {
      choice = keys[0] ?? "simulator";
      picked = false;
    }
  }

  async function connect() {
    await app.connect(choice === "simulator" ? { kind: "simulator" } : { kind: "usb", key: choice });
    picked = false;
    void scan();
  }

  // Look for samplers being switched on or plugged in while nothing is connected.
  $effect(() => {
    if (app.connected) return;
    void scan();
    const timer = setInterval(scan, 2000);
    return () => clearInterval(timer);
  });
  let openProgram = $state<string | undefined>();
  let openMulti = $state<string | undefined>();
  let prefsOpen = $state(false);
  // What the Program and Multi tabs show, and a counter that empties a tab
  // when its item moves to a window of its own.
  let item = $state<{ program?: string; multi?: string }>({});
  let epoch = $state({ program: 0, multi: 0 });

  /** Opens the current view in a window of its own. The front panel and
   *  meters move there; an editor's item does, and the tab empties. */
  async function popOut(kind: WindowKind = view as WindowKind) {
    if (kind === "program" || kind === "multi") {
      const name = view === kind ? item[kind] : undefined;
      await windows.show(kind, name);
      if (name) {
        item[kind] = undefined;
        if (kind === "program") openProgram = undefined;
        else openMulti = undefined;
        epoch[kind]++;
      }
    } else if (kind === "keyboard") await windows.show(kind, app.active?.short_id);
    else await windows.show(kind);
  }

  /** Menu commands (and their shortcuts) for the main window. */
  function command(id: string) {
    const [group, name] = id.split(".");
    if (group === "view") view = name as View;
    else if (id === "app.preferences") prefsOpen = true;
    else if (group === "window" && name !== "main") void popOut(name as WindowKind);
    else if (id === "help.guide") void windows.show("help");
    else if (id === "help.keys") void windows.show("help", undefined, "keys");
  }

  // "Connect whenever one is switched on": look for new samplers every few seconds.
  $effect(() => {
    if (autoConnect() !== "always") return;
    const timer = setInterval(connectAll, 3000);
    return () => clearInterval(timer);
  });

  onMount(async () => {
    await app.init();
    await queue.init();
    await initCommands(command);
    // "Back to main window" in the front panel or meters window.
    await listen<View>("dock", ({ payload }) => (view = payload));
    // For development: #connect=simulator&view=program&program=Drums connects
    // to the simulator and opens a view (and a program, or multi=…).
    const hash = new URLSearchParams(location.hash.slice(1));
    const v = views.find((x) => x.id === hash.get("view"));
    if (hash.get("connect") === "simulator") await app.connect({ kind: "simulator" });
    openProgram = hash.get("program") ?? undefined;
    openMulti = hash.get("multi") ?? undefined;
    if (v) view = v.id;
    if (!hash.has("connect") && autoConnect() !== "off") await connectAll();
    await windows.init();
  });
</script>

<svelte:window onkeydown={onKey} />

<div class="shell">
  <header>
    <div class="brand">s5000-tools</div>
    <nav>
      {#each views as v, i}
        <button class:active={view === v.id} onclick={() => (view = v.id)} title="{v.label} ({modLabel}{i + 1})">
          <span class="bullet {v.bullet}">{i + 1}</span>{v.label}
        </button>
      {/each}
    </nav>
    {#if view !== "transfers"}
      <button
        class="popout"
        onclick={() => popOut()}
        title="Open {views.find((v) => v.id === view)?.label} in its own window"
        aria-label="Open in its own window">⧉</button
      >
    {/if}
    <div class="connection">
      <button class="prefs" onclick={() => (prefsOpen = true)} title="Preferences ({modLabel},)" aria-label="Preferences">Preferences</button>
      {#if app.info}
        <span class="info" title="OS {app.info.os_version}">
          <b>{app.info.name}</b>
          <span>OS {app.info.os_version}</span>
          <span>Wave {app.info.wave_free}% free</span>
          <span>CPU {app.info.cpu_free}% free</span>
        </span>
        <button onclick={() => app.disconnect()} disabled={queue.running}>Disconnect</button>
      {:else}
        <select bind:value={choice} onchange={() => (picked = true)} disabled={app.connecting} aria-label="Sampler">
          {#each usb as d (d.key)}
            <option value={d.key} title={d.label}>{usbLabel(d)}</option>
          {/each}
          <option value="simulator">Simulator</option>
        </select>
        <button class="primary" onclick={connect} disabled={app.connecting}>
          {app.connecting ? "Connecting…" : "Connect"}
        </button>
      {/if}
    </div>
  </header>

  <main>
    {#if view === "transfers"}
      <Transfers />
    {:else if (view === "panel" || view === "meters") && windows.has(view)}
      <div class="elsewhere">
        <p>{view === "panel" ? "The front panel is" : "The meters are"} in a window of their own.</p>
        <button onclick={() => windows.show(view as WindowKind)}>Show the window</button>
      </div>
    {:else if view === "panel"}
      <FrontPanel />
    {:else if view === "meters"}
      <Meters />
    {:else if view === "program"}
      {#key epoch.program}<ProgramEditor initial={openProgram} onitem={(n) => (item.program = n)} />{/key}
    {:else}
      {#key epoch.multi}<MultiEditor initial={openMulti} onitem={(n) => (item.multi = n)} />{/key}
    {/if}
  </main>

  {#if prefsOpen}<Preferences onclose={() => (prefsOpen = false)} />{/if}

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
    grid-template-columns: minmax(0, 1fr);
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
    white-space: nowrap;
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
    min-width: 0;
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
  /* The sampler's details give way first when the window is narrow. */
  .info {
    display: flex;
    gap: 12px;
    font-size: 12px;
    color: var(--sign-muted);
    white-space: nowrap;
    min-width: 0;
    overflow: hidden;
  }
  .info b {
    color: var(--sign-text);
  }
  main {
    min-height: 0;
    overflow: auto;
  }
  .popout {
    border: 1px solid var(--sign-muted);
    background: transparent;
    color: var(--sign-text);
    padding: 1px 7px;
    font-size: 14px;
  }
  .popout:hover:not(:disabled) {
    background: transparent;
    border-color: var(--sign-text);
  }
  .elsewhere {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 10px;
    padding: 24px 16px;
  }
  .elsewhere p {
    margin: 0;
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
