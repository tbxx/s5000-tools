<script lang="ts">
  import { onMount } from "svelte";
  import { api, poll } from "./api";
  import { app } from "./state.svelte";

  // Raw values are dB below full scale; 0x60 (96) means silence.
  const SILENT = 0x60;
  const FLOOR = 60; // dB shown at the bottom of the scale
  const ticks = [0, 6, 12, 18, 24, 30, 40, 50, 60];

  let outputs = $state<number[]>(Array(16).fill(SILENT));
  let inputs = $state<number[]>(Array(2).fill(SILENT));
  let peaks = $state<{ db: number; at: number }[]>(Array.from({ length: 18 }, () => ({ db: SILENT, at: 0 })));

  /** 0 at the floor, 1 at full scale. */
  const height = (db: number) => (db >= SILENT ? 0 : Math.max(0, 1 - db / FLOOR));

  function hold(i: number, db: number, now: number) {
    const p = peaks[i];
    if (db <= p.db || now - p.at > 1500) peaks[i] = { db, at: now };
  }

  onMount(() =>
    poll(60, async () => {
      if (!app.connected) return;
      const m = await api.meters();
      const now = performance.now();
      outputs = m.outputs;
      inputs = m.inputs;
      [...m.outputs, ...m.inputs].forEach((db, i) => hold(i, db, now));
    }),
  );
</script>

{#snippet meter(db: number, peak: number, label: string)}
  <div class="meter">
    <div class="bar">
      <div class="fill" style:height="{height(db) * 100}%"></div>
      {#if peak < SILENT}<div class="peak" style:bottom="{height(peak) * 100}%"></div>{/if}
    </div>
    <span class="label">{label}</span>
    <span class="value">{db >= SILENT ? "—" : `-${db}`}</span>
  </div>
{/snippet}

<div class="wrap">
  <div class="rack">
    <div class="section">
      <h3 class="sign">Outputs</h3>
      <div class="meters">
        <div class="scale">
          {#each ticks as t}<span style:bottom="{(1 - t / FLOOR) * 100}%">{t === 0 ? "0" : `−${t}`}</span>{/each}
        </div>
        {#each outputs as db, i}{@render meter(db, peaks[i].db, String(i + 1))}{/each}
      </div>
    </div>
    <div class="section">
      <h3 class="sign">Record</h3>
      <div class="meters">
        {#each inputs as db, i}{@render meter(db, peaks[16 + i].db, i ? "R" : "L")}{/each}
      </div>
    </div>
  </div>
  <p class="muted note">
    {#if !app.connected}
      Not connected.
    {:else if app.info?.simulated}
      The simulator always reports silence.
    {:else}
      Levels in dB below full scale. Outputs follow the internal routing, so outputs without sockets still show signal.
    {/if}
  </p>
</div>

<style>
  .wrap {
    padding: 18px;
    display: flex;
    flex-direction: column;
    align-items: center;
  }
  .rack {
    display: flex;
    gap: 28px;
  }
  .section {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .section .sign {
    margin: 0;
  }
  .meters {
    display: flex;
    gap: 6px;
    position: relative;
  }
  .scale {
    position: relative;
    width: 26px;
    height: 320px;
    font-size: 9px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .scale span {
    position: absolute;
    right: 4px;
    transform: translateY(50%);
  }
  .scale span::after {
    content: "";
    position: absolute;
    right: -4px;
    top: 50%;
    width: 3px;
    border-top: 1px solid var(--rule);
  }
  .meter {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
  }
  .bar {
    position: relative;
    width: 22px;
    height: 320px;
    background: var(--surface-2);
    border-bottom: 1px solid var(--rule);
    overflow: hidden;
  }
  /* Green to -12 dB, yellow to -3 dB, red above (FLOOR = 60 dB). */
  .fill {
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    background: linear-gradient(
      to top,
      var(--mta-green) 0 80%,
      var(--mta-yellow) 80% 95%,
      var(--mta-red) 95% 100%
    );
    background-size: 100% 320px;
    background-position: bottom;
  }
  .peak {
    position: absolute;
    left: 0;
    right: 0;
    height: 2px;
    background: var(--text);
  }
  .label {
    font-size: 12px;
    font-weight: 700;
  }
  .value {
    font-size: 9px;
    width: 26px;
    text-align: center;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .note {
    font-size: 12px;
    margin-top: 16px;
  }
</style>
