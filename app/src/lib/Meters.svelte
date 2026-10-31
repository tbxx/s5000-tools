<script lang="ts">
  import { onMount } from "svelte";
  import { api, poll } from "./api";
  import { app } from "./state.svelte";
  import Meter from "./meters/Meter.svelte";
  import MeterStylePicker from "./meters/MeterStylePicker.svelte";
  import { channel, step } from "./meters/ballistics";
  import { SILENT, scaleMarks, styleOf, type MeterKind } from "./meters/styles";

  // Raw levels are dB below full scale; 0x60 (96) means silence.
  const RAW_SILENT = 0x60;
  const toDb = (raw: number) => (raw >= RAW_SILENT ? SILENT : -raw);

  const style = $derived(styleOf(app.settings.meterStyle));
  const kind = $derived(style.id);
  const peakHold = $derived(app.settings.meterPeakHold ?? true);
  const vuRef = $derived(app.settings.meterVuRef ?? -18);
  // Outputs 9-16 exist only with the output expander board.
  const outputs = $derived(app.settings.meterShowUpper === false ? 8 : 16);
  const marks = $derived(scaleMarks(kind));

  // 16 outputs, then record L and R. Readings arrive every ~60 ms; the
  // display moves every frame between them.
  const LABELS = [...Array.from({ length: 16 }, (_, i) => String(i + 1)), "L", "R"];
  let readings = Array<number>(18).fill(SILENT);
  let ch = $state(LABELS.map(() => channel()));
  let picking = $state(false);

  function pick(id: MeterKind) {
    app.save({ meterStyle: id });
    picking = false;
  }

  onMount(() => {
    const stop = poll(60, async () => {
      if (!app.connected) {
        readings.fill(SILENT);
        return;
      }
      const m = await api.meters();
      readings = [...m.outputs, ...m.inputs].map(toDb);
    });
    let last = performance.now();
    let raf = requestAnimationFrame(function frame(now) {
      const dt = Math.min(0.1, (now - last) / 1000);
      last = now;
      ch.forEach((c, i) => step(c, readings[i], dt, vuRef));
      raf = requestAnimationFrame(frame);
    });
    return () => {
      stop();
      cancelAnimationFrame(raf);
    };
  });
</script>

<div class="view">
  <div class="tools">
    <button class="style" onclick={() => (picking = !picking)} aria-expanded={picking}>
      Style: {style.name} <span aria-hidden="true">▾</span>
    </button>
    {#if style.family === "led"}
      <label><input type="checkbox" checked={peakHold} onchange={(e) => app.save({ meterPeakHold: e.currentTarget.checked })} /> Peak hold</label>
    {:else}
      <label>
        0 VU =
        <select value={String(vuRef)} onchange={(e) => app.save({ meterVuRef: Number(e.currentTarget.value) })}>
          <option value="-14">−14 dBFS</option>
          <option value="-18">−18 dBFS</option>
          <option value="-20">−20 dBFS</option>
        </select>
      </label>
    {/if}
    <span class="muted note">{style.note}</span>
  </div>
  {#if picking}
    <div class="picker">
      <MeterStylePicker value={kind} onpick={pick} />
    </div>
  {/if}

  {#if style.family === "led"}
    <div class="rack led" class:fsc={kind === "fsc"}>
      <section class="group outputs">
        <h3 class="sign">Outputs</h3>
        <div class="row">
          <div class="scale" aria-hidden="true">
            <div class="marks">
              {#each marks as m}<span style:bottom="{m.at * 100}%">{m.label}</span>{/each}
            </div>
          </div>
          {#each ch.slice(0, outputs) as c, i}
            <div class="cell" class:twin={kind === "twin"}><Meter {kind} ch={c} {peakHold} label={LABELS[i]} /></div>
          {/each}
        </div>
      </section>
      <section class="group record">
        <h3 class="sign">Record</h3>
        <div class="row">
          {#each ch.slice(16) as c, i}
            <div class="cell" class:twin={kind === "twin"}><Meter {kind} ch={c} {peakHold} label={LABELS[16 + i]} /></div>
          {/each}
        </div>
      </section>
    </div>
  {:else}
    <div class="rack vu">
      <section class="group outputs">
        <h3 class="sign">Outputs</h3>
        <div class="grid">
          {#each ch.slice(0, outputs) as c, i}<Meter {kind} ch={c} label={LABELS[i]} />{/each}
        </div>
      </section>
      <section class="group record">
        <h3 class="sign">Record</h3>
        <div class="grid one">
          {#each ch.slice(16) as c, i}<Meter {kind} ch={c} label={LABELS[16 + i]} />{/each}
        </div>
      </section>
    </div>
  {/if}

  <p class="muted foot">
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
  .view {
    height: 100%;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 16px;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 16px;
    flex-wrap: wrap;
    font-size: 12px;
  }
  .tools label {
    display: flex;
    align-items: center;
    gap: 6px;
    font-weight: 700;
  }
  .note {
    flex: 1;
    min-width: 200px;
  }
  .picker {
    padding: 10px;
    border: 1px solid var(--rule);
    background: var(--surface);
  }
  .rack {
    flex: 1;
    min-height: 0;
    display: flex;
    gap: 28px;
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 0;
  }
  .group .sign {
    margin: 0;
  }
  .led .outputs {
    flex: 1;
    min-width: 0;
  }
  .row {
    flex: 1;
    min-height: 200px;
    display: flex;
    gap: 8px;
  }
  .cell {
    flex: 1 1 0;
    min-width: 14px;
    max-width: 60px;
  }
  .cell.twin {
    max-width: 72px;
  }
  .record .cell {
    width: 60px;
    flex: none;
  }
  /* The scale lines up with the bars: room for the OVL lamp above (Studio
     console style) and the label below. */
  .scale {
    width: 26px;
    flex: none;
    display: flex;
    flex-direction: column;
    padding-bottom: 36px;
  }
  .fsc .scale {
    padding-top: 22px;
  }
  .marks {
    flex: 1;
    position: relative;
    font-size: 10px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .marks span {
    position: absolute;
    right: 3px;
    transform: translateY(50%);
  }
  .vu .outputs {
    flex: 1;
    min-width: 0;
    overflow: auto;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 16px;
  }
  .vu .record {
    width: 260px;
    flex: none;
  }
  .grid.one {
    grid-template-columns: minmax(0, 1fr);
  }
  .foot {
    margin: 0;
    font-size: 12px;
  }
</style>
