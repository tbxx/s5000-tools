<script lang="ts">
  // The meter styles as tiles, each previewing itself on a live demo signal.
  import { onMount } from "svelte";
  import Meter from "./Meter.svelte";
  import { demo } from "./demo.svelte";
  import { METER_STYLES, type MeterKind } from "./styles";

  let { value, onpick }: { value: MeterKind; onpick: (kind: MeterKind) => void } = $props();

  onMount(() => demo.use());

  const groups = [
    { title: "LED & plasma", styles: METER_STYLES.filter((s) => s.family === "led") },
    { title: "VU", styles: METER_STYLES.filter((s) => s.family === "vu") },
  ];
</script>

{#each groups as g}
  <div class="group" role="radiogroup" aria-label="{g.title} meter styles">
    <h4>{g.title}</h4>
    <div class="tiles" class:vu={g.title === "VU"}>
      {#each g.styles as s (s.id)}
        <button role="radio" aria-checked={value === s.id} class="tile" class:active={value === s.id} title={s.note} onclick={() => onpick(s.id)}>
          <span class="preview" class:vu={s.family === "vu"}>
            {#each demo.ch as c}
              <span class="cell"><Meter kind={s.id} ch={c} compact /></span>
            {/each}
          </span>
          <span class="name">{s.name}</span>
        </button>
      {/each}
    </div>
  </div>
{/each}

<style>
  .group + .group {
    margin-top: 12px;
  }
  h4 {
    margin: 0 0 6px;
    font-size: 11px;
    color: var(--muted);
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(96px, 1fr));
    gap: 8px;
  }
  .tiles.vu {
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  }
  .tile {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px;
    font-weight: 400;
    text-align: left;
    background: #000;
    color: #fff;
    border: 1px solid var(--rule-soft);
    white-space: normal;
  }
  .tile:hover:not(:disabled) {
    background: #111;
  }
  .tile.active {
    outline: 3px solid var(--accent);
    outline-offset: -3px;
  }
  .preview {
    display: flex;
    justify-content: center;
    gap: 6px;
    height: 92px;
  }
  .preview.vu {
    height: auto;
  }
  .cell {
    width: 18px;
    height: 100%;
  }
  .preview.vu .cell {
    width: auto;
    flex: 1 1 0;
    min-width: 0;
  }
  .name {
    font-size: 11px;
    font-weight: 700;
  }
</style>
