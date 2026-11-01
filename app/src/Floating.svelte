<script lang="ts">
  // A window besides the main one: one view (front panel, meters, a program
  // or multi editor, the keyboard, or Help), with the status line. The front
  // panel and meters can go back into the main window.
  import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
  import { emitTo } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import Dialog from "./lib/Dialog.svelte";
  import FrontPanel from "./lib/FrontPanel.svelte";
  import Help from "./lib/Help.svelte";
  import Meters from "./lib/Meters.svelte";
  import MidiKeyboard from "./lib/MidiKeyboard.svelte";
  import MultiEditor from "./lib/MultiEditor.svelte";
  import ProgramEditor from "./lib/ProgramEditor.svelte";
  import { initCommands, onKey } from "./lib/commands";
  import { app } from "./lib/state.svelte";
  import { here, windows } from "./lib/windows.svelte";

  const kind = here.kind!;

  onMount(async () => {
    await app.init();
    await initCommands();
    await windows.initFloating();
  });

  /** Puts the front panel or meters back in the main window. */
  async function dock() {
    await emitTo("main", "dock", kind);
    await getCurrentWebviewWindow().close();
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="shell">
  {#if kind === "panel" || kind === "meters"}
    <div class="tools">
      <span class="muted">{app.info ? app.label(app.active!.id) : "Not connected"}</span>
      <button class="small" onclick={dock} title="Show it in the main window again">Back to main window</button>
    </div>
  {/if}
  <main>
    {#if kind === "panel"}
      <FrontPanel />
    {:else if kind === "meters"}
      <Meters />
    {:else if kind === "program"}
      <ProgramEditor initial={here.item} onitem={(n) => windows.showing(n)} />
    {:else if kind === "multi"}
      <MultiEditor initial={here.item} onitem={(n) => windows.showing(n)} />
    {:else if kind === "keyboard"}
      <MidiKeyboard />
    {:else}
      <Help section={here.section} />
    {/if}
  </main>
  {#if app.error && kind !== "help"}
    <footer>
      <span class="error">{app.error}</span>
      <button class="link" onclick={() => (app.error = null)}>dismiss</button>
    </footer>
  {/if}
</div>

<Dialog />

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  .tools {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 6px 12px;
    font-size: 12px;
    border-bottom: 1px solid var(--rule-soft);
  }
  .small {
    font-size: 11px;
    padding: 2px 8px;
  }
  main {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  footer {
    display: flex;
    gap: 8px;
    padding: 4px 12px;
    font-size: 12px;
    border-top: 1px solid var(--rule);
  }
  .error {
    color: var(--bad);
    font-weight: 700;
    user-select: text;
    -webkit-user-select: text;
  }
  .link {
    border: none;
    background: none;
    color: var(--muted);
    font-weight: 400;
    text-decoration: underline;
    padding: 0;
  }
</style>
