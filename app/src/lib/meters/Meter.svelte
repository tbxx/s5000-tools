<script lang="ts">
  // One channel's meter in any style. Sized by its container: LED styles fill
  // its height, VU styles its width.
  import { needleVu, type Channel } from "./ballistics";
  import { FLOOR, SILENT, frac, ladder, type MeterKind } from "./styles";
  import { VU_FACE, VU_THEMES } from "./vu";

  let {
    kind,
    ch,
    peakHold = true,
    label = "",
    compact = false,
  }: { kind: MeterKind; ch: Channel; peakHold?: boolean; label?: string; compact?: boolean } = $props();

  const uid = $props.id();
  const isVu = $derived(kind === "ivory" || kind === "amber" || kind === "blackvu");
  const isBar = $derived(kind === "signage" || kind === "plasma");
  const db = $derived(ch.disp);
  const peak = $derived(peakHold ? ch.peak : ch.disp);
  const look = $derived(!isVu && !isBar ? ladder(kind, db, peak, ch.vu) : null);
  const over = $derived(peak >= -0.5);

  const fmt = (d: number) => (d <= SILENT + 1 ? "—" : d >= 0 ? "0.0" : d.toFixed(1));

  // Signage: green, yellow above -12 dB, red above -3 dB, as stops within the fill.
  const fillBg = $derived.by(() => {
    if (kind === "plasma") return "linear-gradient(to top, #ff4d12, #ff6a1f 60%, #ff8a3d)";
    const f = frac(db);
    const at = (x: number) => (f > 0 ? Math.min(100, (frac(x) / f) * 100) : 100).toFixed(2);
    const g = at(-12);
    const y = at(-3);
    return `linear-gradient(to top, var(--mta-green) 0%, var(--mta-green) ${g}%, var(--mta-yellow) ${g}%, var(--mta-yellow) ${y}%, var(--mta-red) ${y}%)`;
  });

  const theme = $derived(isVu ? VU_THEMES[kind as "ivory" | "amber" | "blackvu"] : null);
  const needleDeg = $derived(VU_FACE.angle(Math.max(-0.05, Math.min(1.08, ch.pos))));
  const vuText = $derived.by(() => {
    if (ch.vu <= SILENT + 1) return "—";
    const v = needleVu(ch.pos);
    return `${v >= 0 ? "+" : ""}${v.toFixed(1)} VU`;
  });
</script>

{#if isVu && theme}
  <div class="vu" class:compact>
    <svg viewBox="0 0 400 270" shape-rendering="geometricPrecision" role="img" aria-label="{label} VU meter">
      <defs>
        <linearGradient id="bezel-{uid}" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stop-color={theme.bezel[0]} />
          <stop offset="0.5" stop-color={theme.bezel[1]} />
          <stop offset="1" stop-color={theme.bezel[2]} />
        </linearGradient>
        <radialGradient id="face-{uid}" cx="200" cy={theme.faceCy} r="300" gradientUnits="userSpaceOnUse">
          <stop offset="0" stop-color={theme.face[0]} />
          <stop offset="0.55" stop-color={theme.face[1]} />
          <stop offset="1" stop-color={theme.face[2]} />
        </radialGradient>
        <linearGradient id="glass-{uid}" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stop-color="#fff" stop-opacity="0.34" />
          <stop offset="0.45" stop-color="#fff" stop-opacity="0.06" />
          <stop offset="1" stop-color="#fff" stop-opacity="0" />
        </linearGradient>
        <linearGradient id="inner-{uid}" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stop-color="#000" stop-opacity="0.45" />
          <stop offset="0.08" stop-color="#000" stop-opacity="0" />
        </linearGradient>
        <radialGradient id="hub-{uid}" cx="200" cy="246" r="50" gradientUnits="userSpaceOnUse">
          <stop offset="0" stop-color="#3a3a3a" />
          <stop offset="0.6" stop-color="#151515" />
          <stop offset="1" stop-color="#050505" />
        </radialGradient>
        <radialGradient id="led-{uid}" cx="0.4" cy="0.35" r="0.7">
          <stop offset="0" stop-color={peak >= -6 ? "#ffb3a8" : "#5c2620"} />
          <stop offset="1" stop-color={peak >= -6 ? "#ff2a14" : "#2a0c08"} />
        </radialGradient>
        <clipPath id="win-{uid}"><rect x="18" y="18" width="364" height="234" rx="5" /></clipPath>
      </defs>
      <rect width="400" height="270" rx="12" fill="url(#bezel-{uid})" />
      <rect x="10" y="10" width="380" height="250" rx="8" fill="none" stroke="#000" stroke-opacity="0.6" stroke-width="2" />
      <rect x="11" y="11" width="378" height="248" rx="7" fill="none" stroke="#fff" stroke-opacity="0.08" />
      {#each VU_FACE.screws as s}
        <circle cx={s.x} cy={s.y} r="4.2" fill="#2a2a2a" stroke="#000" stroke-width="0.8" />
        <line x1={s.x1} y1={s.y1} x2={s.x2} y2={s.y2} stroke="#0a0a0a" stroke-width="1.2" />
      {/each}
      <g clip-path="url(#win-{uid})">
        <rect x="18" y="18" width="364" height="234" fill="url(#face-{uid})" />
        <path d={VU_FACE.pctArc} fill="none" stroke={theme.ink} stroke-opacity="0.75" />
        {#each VU_FACE.pctTicks as t}
          <line x1={t.x1} y1={t.y1} x2={t.x2} y2={t.y2} stroke={theme.ink} stroke-opacity="0.75" />
          <text x={t.lx} y={t.ly} fill={theme.ink} fill-opacity="0.75" font-size="11" text-anchor="middle">{t.text}</text>
        {/each}
        <path d={VU_FACE.redBand} fill="none" stroke={theme.red} stroke-width="9" />
        <path d={VU_FACE.mainArc} fill="none" stroke={theme.ink} stroke-width="1.6" />
        <path d={VU_FACE.redArc} fill="none" stroke={theme.red} stroke-width="1.6" />
        {#each VU_FACE.ticks as t}
          <line x1={t.x1} y1={t.y1} x2={t.x2} y2={t.y2} stroke={t.red ? theme.red : theme.ink} stroke-width={t.major ? 2.2 : 1.4} />
        {/each}
        {#each VU_FACE.labels as l}
          <text x={l.x} y={l.y} fill={l.red ? theme.red : theme.ink} font-size="17" font-weight="700" text-anchor="middle">{l.text}</text>
        {/each}
        <text x="200" y="190" fill={theme.ink} font-size="26" font-weight="700" text-anchor="middle" letter-spacing="2">VU</text>
        <!-- SVG's own rotate(): the pivot is in drawing units at any size. A CSS
             transform-origin in px is read in screen pixels, so a scaled meter
             swung its needle around the wrong point. -->
        <g transform="rotate({needleDeg} 203 304)">
          <line x1="203" y1="304" x2="203" y2="78" stroke="#000" stroke-opacity="0.22" stroke-width="3" />
        </g>
        <g transform="rotate({needleDeg} 200 300)">
          <line x1="200" y1="300" x2="200" y2="74" stroke={theme.needle} stroke-width="2.2" stroke-linecap="round" />
        </g>
        <path d="M 146 252 A 54 54 0 0 1 254 252 Z" fill="url(#hub-{uid})" />
        <rect x="18" y="18" width="364" height="234" fill="url(#inner-{uid})" />
        <path d="M 18 18 L 382 18 L 382 96 Q 200 140 18 96 Z" fill="url(#glass-{uid})" />
      </g>
      <rect x="18" y="18" width="364" height="234" rx="5" fill="none" stroke="#000" stroke-width="2" />
      <circle cx="356" cy="44" r="7" fill="url(#led-{uid})" stroke="#000" />
      <text x="356" y="64" fill={theme.ink} fill-opacity="0.7" font-size="9" font-weight="700" text-anchor="middle">PEAK</text>
    </svg>
    {#if !compact}
      <div class="vu-foot"><b>{label}</b><span>{vuText}</span></div>
    {/if}
  </div>
{:else}
  <div class="led" class:compact>
    {#if kind === "fsc"}
      <div class="ovl" class:on={over} title="Overload">OVL</div>
    {/if}
    {#if look}
      <div class="ladder" style:gap="{look.colGap}px" style:padding="{look.pad}px" style:background={look.bg}>
        {#each look.columns as col}
          <div class="col" style:gap="{look.gap}px">
            {#each col as s}
              <div
                class="seg"
                style:background={s.on ? s.color : s.dim}
                style:box-shadow={s.on && look.glow ? `0 0 6px ${s.color}` : "none"}
                style:border-radius="{look.radius}px"
              ></div>
            {/each}
          </div>
        {/each}
      </div>
    {:else}
      <div class="bar" class:plasma={kind === "plasma"}>
        <div class="fill" style:height="{frac(db) * 100}%" style:background={fillBg}></div>
        {#if peak > FLOOR}<div class="peak" style:bottom="{frac(peak) * 100}%"></div>{/if}
      </div>
    {/if}
    {#if !compact}
      <div class="foot"><b>{label}</b><span>{fmt(peak)}</span></div>
    {/if}
  </div>
{/if}

<style>
  .led {
    height: 100%;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .ovl {
    align-self: center;
    height: 16px;
    padding: 1px 4px;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.5px;
    color: #6a3a34;
    background: #2a1210;
    border: 1px solid #3a3a3a;
  }
  .ovl.on {
    color: #fff;
    background: #ff3b30;
    box-shadow: 0 0 8px #ff3b30;
  }
  .ladder {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .col {
    flex: 1 1 0;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .seg {
    flex: 1 1 0;
    min-height: 0;
  }
  .bar {
    flex: 1;
    min-height: 0;
    position: relative;
    background: var(--surface-2);
    overflow: hidden;
  }
  .fill {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
  }
  .peak {
    position: absolute;
    left: 0;
    right: 0;
    height: 2px;
    background: var(--text);
  }
  /* Gas-discharge plasma: a fine-pitched orange-red glow. */
  .bar.plasma {
    background: #160804;
    border: 1px solid #2b120a;
  }
  .plasma .fill {
    box-shadow:
      0 0 12px rgb(255 90 20 / 0.9),
      0 0 26px rgb(255 80 10 / 0.45);
  }
  .plasma .peak {
    height: 3px;
    background: #ffb98a;
    box-shadow: 0 0 10px #ff6a1f;
  }
  .plasma::after {
    content: "";
    position: absolute;
    inset: 0;
    background: repeating-linear-gradient(to top, rgb(10 3 1 / 0.85) 0 1px, transparent 1px 3px);
  }
  .foot {
    height: 30px;
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    font-size: 12px;
  }
  .foot span,
  .vu-foot span {
    font-size: 10px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .vu {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .vu svg {
    display: block;
    width: 100%;
    height: auto;
    font-family: var(--font);
  }
  .vu-foot {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    font-size: 12px;
  }
</style>
