<script lang="ts">
  // The data wheel: drag round it (or scroll over it) to turn it. Clicks are
  // batched and sent at most every 40 ms.
  let { onturn }: { onturn: (clicks: number) => void } = $props();

  const DEG_PER_CLICK = 12;
  let angle = $state(0);
  let el: SVGSVGElement;
  let last: number | null = null;
  let carry = 0;
  let pending = 0;
  let timer: ReturnType<typeof setTimeout> | null = null;

  function queue(clicks: number) {
    pending += clicks;
    timer ??= setTimeout(() => {
      timer = null;
      const n = Math.max(-127, Math.min(127, pending));
      pending = 0;
      if (n) onturn(n);
    }, 40);
  }

  function pointerAngle(e: PointerEvent) {
    const r = el.getBoundingClientRect();
    return (Math.atan2(e.clientY - (r.top + r.height / 2), e.clientX - (r.left + r.width / 2)) * 180) / Math.PI;
  }

  function down(e: PointerEvent) {
    el.setPointerCapture(e.pointerId);
    last = pointerAngle(e);
  }

  function move(e: PointerEvent) {
    if (last === null) return;
    const a = pointerAngle(e);
    let d = a - last;
    if (d > 180) d -= 360;
    if (d < -180) d += 360;
    last = a;
    angle += d;
    carry += d;
    const clicks = Math.trunc(carry / DEG_PER_CLICK);
    if (clicks) {
      carry -= clicks * DEG_PER_CLICK;
      queue(clicks);
    }
  }

  function wheel(e: WheelEvent) {
    e.preventDefault();
    const clicks = e.deltaY < 0 ? 1 : -1;
    angle += clicks * DEG_PER_CLICK;
    queue(clicks);
  }
</script>

<svg
  bind:this={el}
  viewBox="-50 -50 100 100"
  class="wheel"
  role="slider"
  aria-label="Data wheel"
  aria-valuenow={Math.round(angle / DEG_PER_CLICK)}
  tabindex="-1"
  onpointerdown={down}
  onpointermove={move}
  onpointerup={() => (last = null)}
  onpointercancel={() => (last = null)}
  onwheel={wheel}
>
  <circle r="48" class="rim" />
  {#each Array(36) as _, i}
    <line x1="0" y1="-47.5" x2="0" y2="-43" transform="rotate({i * 10 + angle})" class="grip" />
  {/each}
  <circle r="41" class="face" />
  <circle r="5" cx="0" cy="-28" transform="rotate({angle})" class="dimple" />
</svg>

<style>
  /* Flat: hairline rim, fine grip ticks, a solid dimple. */
  .wheel {
    width: 128px;
    height: 128px;
    touch-action: none;
    cursor: grab;
  }
  .rim {
    fill: var(--surface);
    stroke: var(--rule);
    stroke-width: 1;
  }
  .grip {
    stroke: var(--text);
    stroke-width: 0.8;
  }
  .face {
    fill: var(--surface);
    stroke: var(--rule);
    stroke-width: 1;
  }
  .dimple {
    fill: var(--text);
  }
</style>
