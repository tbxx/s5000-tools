<script lang="ts">
  // Envelope shape from attack/decay/sustain/release (0–100), or from four
  // rates and levels for the aux envelope.
  let { points }: { points: [number, number][] | null } = $props();

  const path = $derived(
    points ? points.map(([x, y], i) => `${i ? "L" : "M"} ${(x * 1.6 + 4).toFixed(1)} ${(54 - y * 0.5).toFixed(1)}`).join(" ") : "",
  );
</script>

<svg viewBox="0 0 168 58" class="graph" aria-hidden="true">
  <rect x="0.5" y="0.5" width="167" height="57" rx="4" />
  {#if points}<path d={path} />{:else}<text x="84" y="33">—</text>{/if}
</svg>

<style>
  .graph {
    width: 168px;
    height: 58px;
  }
  rect {
    fill: var(--lcd-bg);
    stroke: var(--lcd-edge);
  }
  path {
    fill: none;
    stroke: var(--lcd-fg);
    stroke-width: 1.6;
    stroke-linejoin: round;
  }
  text {
    fill: var(--lcd-fg);
    font-size: 12px;
    text-anchor: middle;
  }
</style>
