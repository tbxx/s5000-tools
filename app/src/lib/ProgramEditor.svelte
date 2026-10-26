<script lang="ts">
  // Program editor: every program, keygroup and zone parameter in labelled
  // panels. Labels, ranges and value names come from spec/params.toml; values
  // come from the program's .akp, plus a live read of what the file lacks.
  // Programs in the sampler's memory can be edited live.
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import Control from "./Control.svelte";
  import EnvGraph from "./EnvGraph.svelte";
  import { api, message, type ParamInfo, type ProgramView, type Values } from "./api";
  import { noteName } from "./format";
  import { app } from "./state.svelte";

  let { initial }: { initial?: string } = $props();

  /** A parameter ("id" or "id#n"), or a mod source with its amount. */
  type Cell = string | { src: string; amt: string; label: string };
  const mod = (src: string, amt: string, label: string): Cell => ({ src, amt, label });
  interface Panel {
    title: string;
    cells: Cell[];
    env?: "amp" | "filter" | "aux";
  }

  const lfo = (n: 1 | 2): Cell[] => [
    `program.lfo_rate#${n}`,
    `program.lfo_delay#${n}`,
    `program.lfo_depth#${n}`,
    `program.lfo_waveform#${n}`,
    mod(`program.lfo_rate_mod_source#${n}`, `program.lfo_rate_mod_amount#${n}`, "Rate mod"),
    mod(`program.lfo_delay_mod_source#${n}`, `program.lfo_delay_mod_amount#${n}`, "Delay mod"),
    mod(`program.lfo_depth_mod_source#${n}`, `program.lfo_depth_mod_amount#${n}`, "Depth mod"),
  ];

  const programPanels: Panel[] = [
    {
      title: "Program tune",
      cells: ["program.semitone", "program.fine", "program.portamento", "program.portamento_time", "program.portamento_mode", "program.legato"],
    },
    { title: "Pitchbend", cells: ["program.bend_up", "program.bend_down", "program.bend_mode", "program.aftertouch_pitch"] },
    {
      title: "Output level",
      cells: [
        "program.loudness",
        "program.velocity_sensitivity",
        mod("program.amp_mod_source#1", "program.amp_mod_amount#1", "Amp mod 1"),
        mod("program.amp_mod_source#2", "program.amp_mod_amount#2", "Amp mod 2"),
      ],
    },
    {
      title: "Pan mod",
      cells: [1, 2, 3].map((n) => mod(`program.pan_mod_source#${n}`, `program.pan_mod_amount#${n}`, `Pan mod ${n}`)),
    },
    { title: "Tuning", cells: ["program.tune_template", "program.tune_key", "program.keygroup_crossfade"] },
    { title: "LFO 1", cells: [...lfo(1), "program.lfo1_key_sync#1", "program.lfo1_wheel#1", "program.lfo1_aftertouch#1"] },
    { title: "LFO 2", cells: [...lfo(2), "program.lfo2_retrigger#2", "program.lfo2_clock_sync#2", "program.lfo2_clock_division#2"] },
  ];

  const envCells = (e: "amp" | "filter"): Cell[] =>
    ["attack", "decay", "sustain", "release", "velocity_attack", "keyscale", "velocity_release", "off_release"].map(
      (f) => `keygroup.${e}_env_${f}`,
    );

  const keygroupPanels: Panel[] = [
    {
      title: "KG pitch / amp",
      cells: [
        "keygroup.semitone",
        "keygroup.fine",
        "keygroup.level",
        mod("program.kg_pitch_mod_source#1", "keygroup.pitch_mod_amount#1", "Pitch mod 1"),
        mod("program.kg_pitch_mod_source#2", "keygroup.pitch_mod_amount#2", "Pitch mod 2"),
        mod("program.kg_amp_mod_source#1", "keygroup.amp_mod_amount#1", "Amp mod"),
        "keygroup.fx_override",
        "keygroup.fx_send",
        "keygroup.mute_group",
        "keygroup.velocity_crossfade",
      ],
    },
    {
      title: "Filter",
      cells: [
        "keygroup.filter_type",
        "keygroup.cutoff",
        "keygroup.resonance",
        "keygroup.filter_keyboard_tracking",
        "keygroup.filter_attenuation",
        mod("program.filter_mod_source#1", "keygroup.filter_mod_amount#1", "Filter mod 1"),
        mod("program.filter_mod_source#2", "keygroup.filter_mod_amount#2", "Filter mod 2"),
        mod("program.filter_mod_source#3", "keygroup.filter_mod_amount#3", "Filter mod 3"),
      ],
    },
    { title: "Filter env", env: "filter", cells: [...envCells("filter"), "keygroup.filter_env_depth"] },
    { title: "Amp env", env: "amp", cells: envCells("amp") },
    {
      title: "Aux env",
      env: "aux",
      cells: [
        ...[1, 2, 3, 4].map((n) => `keygroup.aux_env_rate#${n}`),
        ...[1, 2, 3, 4].map((n) => `keygroup.aux_env_level#${n}`),
        "keygroup.aux_env_velocity_r1#1",
        "keygroup.aux_env_velocity_r4#4",
        "keygroup.aux_env_keyscale",
        "keygroup.aux_env_off_release#4",
      ],
    },
  ];

  const zoneCells: Cell[] = [
    "zone.level",
    "zone.pan",
    "zone.semitone",
    "zone.fine",
    "zone.filter",
    "zone.output",
    "zone.playback",
    "zone.keyboard_tracking",
    "zone.velocity_start",
    "zone.low_velocity",
    "zone.high_velocity",
    "zone.mute",
    "zone.solo",
  ];

  let catalog = $state<Map<string, ParamInfo>>(new Map());
  let view = $state<ProgramView | null>(null);
  /** Where the program came from: a name in memory, or a file. */
  let source = $state<{ memory: string } | { file: string } | null>(null);
  let live = $state<Values>({});
  let kg = $state(0);
  let zone = $state(0);
  let loading = $state(false);
  let picked = $state("");

  const editable = $derived(!!source && "memory" in source && app.connected);
  const keygroup = $derived(view?.keygroups[kg]);

  /** Parameters not placed in any panel, shown under "Other". */
  const other = $derived.by(() => {
    const placed = new Set(
      [...programPanels, ...keygroupPanels, { title: "", cells: zoneCells }]
        .flatMap((p) => p.cells.flatMap((c) => (typeof c === "string" ? [c] : [c.src, c.amt])))
        .map((c) => c.split("#")[0]),
    );
    return [...catalog.values()].filter(
      (p) => !placed.has(p.id) && p.encoding && p.encoding !== "string" && p.scope !== "zone",
    );
  });

  onMount(async () => {
    try {
      catalog = new Map((await api.paramCatalog()).map((p) => [p.id, p]));
    } catch (e) {
      app.fail(e);
    }
    if (initial) {
      picked = initial;
      await loadFromMemory(initial);
    }
  });

  async function loadFromMemory(name: string) {
    if (!name) return;
    loading = true;
    try {
      view = await api.readProgram(name);
      source = { memory: name };
      kg = 0;
      zone = 0;
      await readLive();
    } catch (e) {
      app.fail(e);
    } finally {
      loading = false;
    }
  }

  async function loadFile() {
    const path = await open({ filters: [{ name: "Programs", extensions: ["akp", "AKP"] }], defaultPath: app.settings.folder });
    if (typeof path !== "string") return;
    try {
      view = await api.readProgramFile(path);
      source = { file: path };
      live = {};
      kg = 0;
      zone = 0;
      picked = "";
    } catch (e) {
      app.fail(e);
    }
  }

  async function readLive() {
    live = {};
    if (!source || !("memory" in source) || !app.connected) return;
    try {
      live = await api.readLive(source.memory, kg);
    } catch (e) {
      app.fail(`Live values: ${message(e)}`);
    }
  }

  function selectKeygroup(i: number) {
    kg = i;
    readLive();
  }

  /** The value of `ref` ("id" or "id#n"), from the file or the live read. */
  function valueOf(ref: string): number | undefined {
    if (!view) return undefined;
    const info = catalog.get(ref.split("#")[0]);
    if (!info) return undefined;
    if (info.scope === "zone") return keygroup?.zones[zone]?.values[ref] ?? live[`${ref}#${zone + 1}`];
    const file = info.scope === "program" ? view.values : keygroup?.values;
    return file?.[ref] ?? live[ref];
  }

  async function write(ref: string, value: number) {
    if (!view || !source || !("memory" in source)) return;
    const [id, n] = ref.split("#");
    const info = catalog.get(id)!;
    const selector = info.scope === "zone" ? zone + 1 : n ? parseInt(n) : null;
    // Show the new value at once.
    if (info.scope === "zone" && keygroup?.zones[zone] && ref in keygroup.zones[zone].values)
      keygroup.zones[zone].values[ref] = value;
    else if (info.scope === "program" && ref in view.values) view.values[ref] = value;
    else if (info.scope === "keygroup" && keygroup && ref in keygroup.values) keygroup.values[ref] = value;
    else live[info.scope === "zone" ? `${ref}#${zone + 1}` : ref] = value;
    try {
      await api.writeParam(source.memory, id, info.scope === "program" ? null : kg, selector, value);
    } catch (e) {
      app.fail(`${info.label}: ${message(e)}`);
    }
  }

  function envPoints(e: "amp" | "filter" | "aux"): [number, number][] | null {
    if (e === "aux") {
      const r = [1, 2, 3, 4].map((n) => valueOf(`keygroup.aux_env_rate#${n}`));
      const l = [1, 2, 3, 4].map((n) => valueOf(`keygroup.aux_env_level#${n}`));
      if ([...r, ...l].some((v) => v === undefined)) return null;
      const w = (v: number) => (v / 100) * 22;
      const x1 = w(r[0]!), x2 = x1 + w(r[1]!), x3 = x2 + w(r[2]!);
      return [[0, 0], [x1, l[0]!], [x2, l[1]!], [x3, l[2]!], [x3 + 12, l[2]!], [x3 + 12 + w(r[3]!), l[3]!]];
    }
    const [a, d, s, r] = ["attack", "decay", "sustain", "release"].map((f) => valueOf(`keygroup.${e}_env_${f}`));
    if ([a, d, s, r].some((v) => v === undefined)) return null;
    const w = (v: number) => (v / 100) * 30;
    const x1 = w(a!), x2 = x1 + w(d!), x3 = x2 + 20;
    return [[0, 0], [x1, 100], [x2, s!], [x3, s!], [x3 + w(r!), 0]];
  }

  /** Labels without the panel's own name ("Amp env attack" in Amp env is "Attack"). */
  function shortLabel(info: ParamInfo, ref: string) {
    const n = ref.split("#")[1];
    // Number repeated controls (aux env rate 1–4), but not LFO 1/2, which have their own panels.
    const suffix = n && info.index && info.index[0] !== "lfo" && info.index[2] > info.index[1] ? ` ${n}` : "";
    const s = info.label.replace(/^(LFO\d? |LFO\d MIDI |Filter env |Amp env |Aux env |Keygroup |Pitch bend )/, "")
      .replace(/ tune$/, "").replace(/ amount$/, "");
    return s.charAt(0).toUpperCase() + s.slice(1) + suffix;
  }

  // Keyboard strip, A-1 (21) to G8 (127): white keys with black keys on top.
  const LOW = 21;
  const HIGH = 127;
  const isBlack = (n: number) => [1, 3, 6, 8, 10].includes(n % 12);
  const whites = [...Array(HIGH - LOW + 1).keys()].map((i) => LOW + i).filter((n) => !isBlack(n));
  /** Left edge and width of a key, in white-key units. */
  function keyX(n: number): [number, number] {
    const w = whites.filter((m) => m < n).length;
    return isBlack(n) ? [w - 0.3, 0.6] : [w, 1];
  }
  const ranges = $derived(
    view?.keygroups.map((k) => [k.values["keygroup.low_key"] ?? LOW, k.values["keygroup.high_key"] ?? HIGH]) ?? [],
  );
</script>

{#snippet cell(c: Cell)}
  {#if typeof c === "string"}
    {@render one(c)}
  {:else}
    <div class="stack">
      {@render one(c.amt, c.label)}
      {@render one(c.src, "")}
    </div>
  {/if}
{/snippet}

{#snippet one(ref: string, label?: string)}
  {@const info = catalog.get(ref.split("#")[0])}
  {#if info}
    <Control
      {info}
      value={valueOf(ref)}
      label={label ?? shortLabel(info, ref)}
      onchange={editable ? (v) => write(ref, v) : undefined}
    />
  {/if}
{/snippet}

{#snippet panel(p: Panel)}
  <section class="panel">
    <span class="title">{p.title}</span>
    {#if p.env}<EnvGraph points={envPoints(p.env)} />{/if}
    <div class="cells">
      {#each p.cells as c}{@render cell(c)}{/each}
    </div>
  </section>
{/snippet}

<div class="editor">
  <div class="bar">
    <label>
      Program in memory
      <select bind:value={picked} onchange={() => loadFromMemory(picked)} disabled={!app.connected || loading}>
        <option value="">—</option>
        {#each app.memory.programs as p}<option value={p.name}>{p.name}</option>{/each}
      </select>
    </label>
    <button onclick={loadFile}>Open .akp file…</button>
    {#if view}
      <span class="lcd name">{view.name}</span>
      <span class="muted">
        {view.keygroups.length} keygroup{view.keygroups.length === 1 ? "" : "s"} ·
        {editable ? "editing the sampler's memory live" : "read-only (file)"}
        {#if editable && app.info?.simulated}· the simulator does not take parameter edits{/if}
      </span>
    {/if}
  </div>

  {#if !view}
    <p class="empty muted">
      Choose a program in the sampler's memory, or open a saved .akp file. Values greyed out are not stored in the
      file{app.connected ? "" : "; connect to read them live"}.
    </p>
  {:else}
    <div class="grid program">
      {#each programPanels as p}{@render panel(p)}{/each}
    </div>

    <div class="keys">
      <svg viewBox="0 0 {whites.length} 9" preserveAspectRatio="none" class="keyboard" aria-label="Keygroup ranges">
        {#each whites as n}<rect x={keyX(n)[0]} y="0" width="1" height="6" class="white" />{/each}
        {#each whites as n}
          {#if isBlack(n + 1) && n + 1 <= HIGH}<rect x={keyX(n + 1)[0]} y="0" width="0.6" height="3.8" class="black" />{/if}
        {/each}
        {#each ranges as [lo, hi], i}
          <rect
            x={keyX(lo)[0]}
            y={i === kg ? 6.4 : 7.2}
            width={keyX(hi)[0] + keyX(hi)[1] - keyX(lo)[0]}
            height={i === kg ? 2.4 : 1.2}
            class="range"
            class:current={i === kg}
            role="button"
            tabindex="-1"
            onclick={() => selectKeygroup(i)}
            onkeydown={() => {}}
          />
        {/each}
      </svg>
      <div class="octaves">
        {#each Array(9) as _, o}<span style:left="{(keyX(24 + o * 12)[0] / whites.length) * 100}%">C{o}</span>{/each}
      </div>
    </div>

    <div class="tabs">
      {#each view.keygroups as _, i}
        <button class:active={i === kg} onclick={() => selectKeygroup(i)}>{i + 1}</button>
      {/each}
      {#if keygroup}
        <span class="muted range-text">
          {noteName(ranges[kg][0])} – {noteName(ranges[kg][1])}
        </span>
      {/if}
    </div>

    <div class="grid keygroup">
      {#each keygroupPanels as p}{@render panel(p)}{/each}
      <section class="panel zone">
        <span class="title">Zone</span>
        <div class="zone-tabs">
          {#each keygroup?.zones ?? [] as z, i}
            <button class:active={i === zone} class:empty-zone={!z.sample} onclick={() => (zone = i)}>{i + 1}</button>
          {/each}
          <span class="lcd sample">{keygroup?.zones[zone]?.sample ?? "(no sample)"}</span>
        </div>
        <div class="cells">
          {#each zoneCells as c}{@render cell(c)}{/each}
        </div>
      </section>
    </div>

    {#if other.length}
      <section class="panel other">
        <span class="title">Other</span>
        <div class="cells">
          {#each other as p}
            {#if p.index}
              {#each Array(p.index[2] - p.index[1] + 1) as _, i}{@render one(`${p.id}#${p.index[1] + i}`)}{/each}
            {:else}
              {@render one(p.id)}
            {/if}
          {/each}
        </div>
      </section>
    {/if}
  {/if}
</div>

<style>
  .editor {
    padding: 10px 12px 16px;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 10px;
  }
  .name {
    font-size: 13px;
    padding: 2px 10px;
  }
  .empty {
    padding: 30px;
  }
  .grid {
    display: grid;
    gap: 8px;
  }
  .grid.program {
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
  }
  .grid.keygroup {
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
  }
  .panel {
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 6px 8px 8px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
  }
  .panel.other {
    margin-top: 8px;
  }
  .cells {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 6px 4px;
  }
  .stack {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
  }
  .keys {
    position: relative;
    margin: 12px 0 2px;
    padding-bottom: 14px;
  }
  .keyboard {
    width: 100%;
    height: 46px;
    display: block;
  }
  .keyboard .white {
    fill: #f7f6f2;
    stroke: #8f8a80;
    stroke-width: 0.06;
  }
  .keyboard .black {
    fill: #2a2c30;
  }
  .keyboard .range {
    fill: var(--muted);
    opacity: 0.55;
    stroke: none;
  }
  .keyboard .range.current {
    fill: var(--accent);
    opacity: 1;
  }
  .octaves span {
    position: absolute;
    bottom: 0;
    font-size: 9px;
    color: var(--muted);
    transform: translateX(-2px);
  }
  .tabs,
  .zone-tabs {
    display: flex;
    align-items: center;
    gap: 3px;
    flex-wrap: wrap;
  }
  .tabs {
    margin: 4px 0 8px;
  }
  .tabs button,
  .zone-tabs button {
    padding: 0 7px;
    font-size: 11px;
    min-width: 26px;
  }
  .tabs button.active,
  .zone-tabs button.active {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
  }
  .empty-zone {
    opacity: 0.55;
  }
  .range-text {
    margin-left: 8px;
    font-size: 12px;
  }
  .sample {
    margin-left: 8px;
    min-width: 140px;
  }
</style>
