<script lang="ts">
  import { dialogs, type Field } from "./dialogs.svelte";

  let values = $state<string[]>([]);
  let form = $state<HTMLFormElement>();

  $effect(() => {
    const d = dialogs.current;
    if (!d) return;
    values = d.fields.map((f: Field) => f.value);
    queueMicrotask(() => (form?.querySelector("input, select, button.primary") as HTMLElement | null)?.focus());
  });

  function submit(e: SubmitEvent) {
    e.preventDefault();
    dialogs.close($state.snapshot(values));
  }
</script>

{#if dialogs.current}
  {@const d = dialogs.current}
  <div class="scrim" role="presentation" onkeydown={(e) => e.key === "Escape" && dialogs.close(null)}>
    <form class="dialog" bind:this={form} onsubmit={submit}>
      <h2 class="sign">{d.title}</h2>
      <div class="body">
      {#if d.text}<p>{d.text}</p>{/if}
      {#each d.fields as field, i}
        <label class="field">
          <span>{field.label}</span>
          {#if field.options}
            <select bind:value={values[i]}>
              {#each field.options as o}<option value={o}>{o}</option>{/each}
            </select>
          {:else}
            <input bind:value={values[i]} maxlength="20" spellcheck="false" />
          {/if}
        </label>
      {/each}
      <div class="buttons">
        <button type="button" onclick={() => dialogs.close(null)}>Cancel</button>
        <button type="submit" class="primary" class:danger={d.danger}>{d.ok}</button>
      </div>
      </div>
    </form>
  </div>
{/if}

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
    min-width: 360px;
    max-width: 480px;
  }
  h2 {
    margin: 0;
    font-size: 14px;
    padding: 6px 12px;
  }
  .body {
    padding: 14px 12px 12px;
  }
  p {
    margin: 0 0 12px;
    white-space: pre-line;
    -webkit-user-select: text;
    user-select: text;
  }
  .field {
    display: grid;
    grid-template-columns: 90px 1fr;
    margin-bottom: 8px;
    font-weight: 700;
    font-size: 12px;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 14px;
    padding-top: 12px;
    border-top: 1px solid var(--rule-soft);
  }
  .danger {
    background: var(--bad);
    border-color: var(--bad);
    color: #fff;
  }
</style>
