<script lang="ts">
  import { app } from "./state.svelte";
  import { ZOOMS, autoConnect, connectAll, modLabel, setZoom, zoom, type AutoConnect } from "./prefs.svelte";

  let { onclose }: { onclose: () => void } = $props();

  const choices: { value: AutoConnect; label: string; detail: string }[] = [
    { value: "off", label: "Off", detail: "Connect with the menu at the top right." },
    { value: "startup", label: "When the app starts", detail: "Every S5000/S6000 found on USB." },
    {
      value: "always",
      label: "When the app starts and whenever one is switched on",
      detail: "A sampler you disconnect stays disconnected until it is switched off or unplugged.",
    },
  ];

  function setAuto(v: AutoConnect) {
    app.save({ autoConnect: v });
    if (v !== "off") void connectAll();
  }

  let dialog = $state<HTMLElement>();
  $effect(() => dialog?.focus());
</script>

<div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby="prefs-title"
    tabindex="-1"
    bind:this={dialog}
    onkeydown={(e) => e.key === "Escape" && onclose()}
  >
    <h2 class="sign" id="prefs-title">Preferences</h2>
    <div class="body">
      <section>
        <h3>Size</h3>
        <div class="zooms" role="radiogroup" aria-label="Interface size">
          {#each ZOOMS as z}
            <button role="radio" aria-checked={Math.abs(zoom() - z) < 0.001} class:active={Math.abs(zoom() - z) < 0.001} onclick={() => setZoom(z)}>
              {Math.round(z * 100)}%
            </button>
          {/each}
        </div>
        <p class="hint">{modLabel}+ and {modLabel}− change the size from anywhere; {modLabel}0 resets it.</p>
      </section>

      <section>
        <h3>Connect to samplers automatically</h3>
        {#each choices as c}
          <label class="choice">
            <input type="radio" name="auto" checked={autoConnect() === c.value} onchange={() => setAuto(c.value)} />
            <span><b>{c.label}</b><small>{c.detail}</small></span>
          </label>
        {/each}
      </section>

      <div class="buttons">
        <button class="primary" onclick={onclose}>Done</button>
      </div>
    </div>
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.4);
    display: grid;
    place-items: center;
    z-index: 10;
  }
  .dialog {
    background: var(--surface);
    border: 1px solid var(--rule);
    width: min(480px, calc(100vw - 32px));
    outline: none;
  }
  h2 {
    margin: 0;
    font-size: 14px;
    padding: 6px 12px;
  }
  .body {
    padding: 4px 12px 12px;
  }
  section {
    padding: 10px 0 12px;
    border-bottom: 1px solid var(--rule-soft);
  }
  h3 {
    margin: 0 0 8px;
    font-size: 12px;
  }
  .zooms {
    display: flex;
    flex-wrap: wrap;
  }
  .zooms button {
    min-width: 52px;
    margin-left: -1px;
  }
  .zooms button.active {
    background: var(--text);
    color: var(--bg);
    border-color: var(--text);
  }
  .hint {
    margin: 8px 0 0;
    color: var(--muted);
    font-size: 11px;
  }
  .choice {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    padding: 4px 0;
    font-size: 12px;
  }
  .choice input {
    margin-top: 2px;
  }
  .choice span {
    display: grid;
    gap: 2px;
  }
  .choice small {
    color: var(--muted);
    font-size: 11px;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    padding-top: 12px;
  }
</style>
