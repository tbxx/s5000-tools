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
      <h2>{d.title}</h2>
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
    </form>
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.35);
    display: grid;
    place-items: center;
    z-index: 10;
  }
  .dialog {
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 16px 18px;
    min-width: 340px;
    max-width: 480px;
    box-shadow: 0 10px 40px rgb(0 0 0 / 0.3);
  }
  h2 {
    font-size: 14px;
    margin: 0 0 8px;
  }
  p {
    margin: 0 0 12px;
    color: var(--muted);
    white-space: pre-line;
    -webkit-user-select: text;
    user-select: text;
  }
  .field {
    display: grid;
    grid-template-columns: 90px 1fr;
    margin-bottom: 8px;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 12px;
  }
  .danger {
    background: var(--bad);
    border-color: var(--bad);
  }
</style>
