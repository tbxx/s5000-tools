<script lang="ts">
  // Colour schemes as tiles, each a miniature of the app in that scheme.
  import { SCHEMES, type SchemeId } from "./schemes";

  let { value, onpick }: { value: SchemeId; onpick: (id: SchemeId) => void } = $props();
</script>

{#snippet mini()}
  <span class="mini">
    <span class="msign"><i class="b blue"></i><i class="b red"></i><i class="b green"></i></span>
    <span class="mbody">
      <span class="mrow"></span>
      <span class="mrow sel"></span>
      <span class="mrow"></span>
      <span class="mfoot"><span class="mbtn"></span><span class="mlcd">S5000</span></span>
    </span>
  </span>
{/snippet}

<div class="tiles" role="radiogroup" aria-label="Colour scheme">
  {#each SCHEMES as s (s.id)}
    <button role="radio" aria-checked={value === s.id} class="tile" class:active={value === s.id} onclick={() => onpick(s.id)}>
      {#if s.id === "system"}
        <span class="split">
          <span data-theme="light">{@render mini()}</span>
          <span data-theme="dark">{@render mini()}</span>
        </span>
      {:else}
        <span class="whole" data-theme={s.id}>{@render mini()}</span>
      {/if}
      <span class="name">{s.name}</span>
    </button>
  {/each}
</div>

<style>
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
    gap: 8px;
  }
  .tile {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 6px;
    font-weight: 400;
    text-align: left;
    white-space: normal;
  }
  .tile.active {
    outline: 3px solid var(--accent);
    outline-offset: -3px;
  }
  .split,
  .whole {
    align-self: stretch;
    width: 100%;
  }
  .split {
    display: grid;
    grid-template-columns: 1fr 1fr;
    height: 72px;
    overflow: hidden;
    border: 1px solid var(--rule-soft);
  }
  .split > span {
    overflow: hidden;
  }
  .whole {
    display: block;
    height: 72px;
    border: 1px solid var(--rule-soft);
  }
  /* The miniature: a signage bar, three rows (one selected), a button and the LCD. */
  .mini {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg);
    color: var(--text);
  }
  .msign {
    display: flex;
    gap: 3px;
    padding: 4px;
    background: var(--sign-bg);
  }
  .b {
    width: 7px;
    height: 7px;
    border-radius: 50%;
  }
  .b.blue {
    background: var(--mta-blue);
  }
  .b.red {
    background: var(--mta-red);
  }
  .b.green {
    background: var(--mta-green);
  }
  .mbody {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 4px;
  }
  .mrow {
    height: 7px;
    border-bottom: 1px solid var(--rule-soft);
  }
  .mrow.sel {
    background: var(--select);
  }
  .mfoot {
    margin-top: auto;
    display: flex;
    gap: 4px;
    align-items: center;
  }
  .mbtn {
    width: 22px;
    height: 9px;
    border: 1px solid var(--rule);
  }
  .mlcd {
    font-size: 7px;
    font-weight: 700;
    padding: 0 3px;
    background: var(--lcd-bg);
    color: var(--lcd-fg);
    border: 1px solid var(--lcd-edge);
  }
  .name {
    font-size: 11px;
    font-weight: 700;
  }
</style>
