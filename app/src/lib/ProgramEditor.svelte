<script lang="ts">
  // Program editor: every program, keygroup and zone parameter in labelled
  // panels. Labels, ranges and value names come from spec/params.toml; values
  // come from the program's .akp, plus a live read of what the file lacks.
  // Programs in the sampler's memory can be edited live.
  import { onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import { open } from "@tauri-apps/plugin-dialog";
  import Control from "./Control.svelte";
  import EnvGraph from "./EnvGraph.svelte";
  import { api, message, type ParamInfo, type ProgramView, type Values } from "./api";
  import { noteName } from "./format";
  import { rove } from "./keys";
  import { modKey, modLabel } from "./prefs.svelte";
  import { app } from "./state.svelte";

  let { initial, onitem }: { initial?: string; onitem?: (name?: string) => void } = $props();

  /** A parameter ("id" or "id#n"), or a mod source with its amount. */
  type Cell = string | { src: string; amt: string; label: string };
  const mod = (src: string, amt: string, label: string): Cell => ({ src, amt, label });
  /** Route colours by function: pitch blue, amp red, filter green, LFO purple, aux orange. */
  type Route = "blue" | "red" | "green" | "purple" | "orange" | "brown" | "lime" | "grey";
  interface Panel {
    title: string;
    route: Route;
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
      title: "Program tune", route: "blue",
      cells: ["program.semitone", "program.fine", "program.portamento", "program.portamento_time", "program.portamento_mode", "program.legato"],
    },
    { title: "Pitchbend", route: "blue", cells: ["program.bend_up", "program.bend_down", "program.bend_mode", "program.aftertouch_pitch"] },
    {
      title: "Output level", route: "red",
      cells: [
        "program.loudness",
        "program.velocity_sensitivity",
        mod("program.amp_mod_source#1", "program.amp_mod_amount#1", "Amp mod 1"),
        mod("program.amp_mod_source#2", "program.amp_mod_amount#2", "Amp mod 2"),
      ],
    },
    {
      title: "Pan mod", route: "red",
      cells: [1, 2, 3].map((n) => mod(`program.pan_mod_source#${n}`, `program.pan_mod_amount#${n}`, `Pan mod ${n}`)),
    },
    { title: "Tuning", route: "brown", cells: ["program.tune_template", "program.tune_key", "program.keygroup_crossfade"] },
    { title: "LFO 1", route: "purple", cells: [...lfo(1), "program.lfo1_key_sync#1", "program.lfo1_wheel#1", "program.lfo1_aftertouch#1"] },
    { title: "LFO 2", route: "purple", cells: [...lfo(2), "program.lfo2_retrigger#2", "program.lfo2_clock_sync#2", "program.lfo2_clock_division#2"] },
  ];

  const envCells = (e: "amp" | "filter"): Cell[] =>
    ["attack", "decay", "sustain", "release", "velocity_attack", "keyscale", "velocity_release", "off_release"].map(
      (f) => `keygroup.${e}_env_${f}`,
    );

  const keygroupPanels: Panel[] = [
    {
      title: "KG pitch / amp", route: "blue",
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
      title: "Filter", route: "green",
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
    { title: "Filter env", route: "green", env: "filter", cells: [...envCells("filter"), "keygroup.filter_env_depth"] },
    { title: "Amp env", route: "red", env: "amp", cells: envCells("amp") },
    {
      title: "Aux env", route: "orange",
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
  /** Selected keygroups (always including `kg`). Keygroup and zone edits go
   *  to each of them (the zone: the same zone number in each), like the
   *  sampler's "ALL" keygroup. Back to one keygroup for each program. */
  const chosen = new SvelteSet<number>([0]);
  const allKeygroups = $derived(!!view && view.keygroups.length > 1 && chosen.size === view.keygroups.length);

  /** Click a keygroup: just it. ⌘-click (Ctrl elsewhere): add or remove it.
   *  ⇧-click: the range from the current keygroup. */
  function pickKeygroup(e: MouseEvent, i: number) {
    if (e.shiftKey) {
      const [a, b] = [Math.min(kg, i), Math.max(kg, i)];
      chosen.clear();
      for (let k = a; k <= b; k++) chosen.add(k);
    } else if (modKey(e)) {
      if (chosen.has(i) && chosen.size > 1) {
        chosen.delete(i);
        if (kg === i) selectKeygroup([...chosen][0], false);
        return;
      }
      chosen.add(i);
    } else {
      chosen.clear();
      chosen.add(i);
    }
    selectKeygroup(i, false);
  }

  /** Edit all: every keygroup, or back to the current one. */
  function setAll(on: boolean) {
    chosen.clear();
    if (on) view?.keygroups.forEach((_, i) => chosen.add(i));
    else chosen.add(kg);
  }
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
  });

  // Opened on a program: load it once it is in memory (a window of its own
  // may open before the sampler is connected).
  let started = false;
  $effect(() => {
    if (started || !initial || !app.connected || !app.memory.programs.some((p) => p.name === initial)) return;
    started = true;
    picked = initial;
    void loadFromMemory(initial);
  });

  async function loadFromMemory(name: string) {
    if (!name) return;
    loading = true;
    try {
      view = await api.readProgram(name);
      source = { memory: name };
      chosen.clear();
      chosen.add(0);
      onitem?.(name);
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
      onitem?.(undefined);
      chosen.clear();
      chosen.add(0);
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

  /** ⌥↑/⌥↓: previous/next keygroup. ⌥←/⌥→: previous/next zone. */
  function editorKey(e: KeyboardEvent) {
    if (!e.altKey || e.metaKey || e.ctrlKey || !view) return;
    const zones = keygroup?.zones.length ?? 0;
    if (e.key === "ArrowUp" && kg > 0) selectKeygroup(kg - 1);
    else if (e.key === "ArrowDown" && kg < view.keygroups.length - 1) selectKeygroup(kg + 1);
    else if (e.key === "ArrowLeft" && zone > 0) zone--;
    else if (e.key === "ArrowRight" && zone < zones - 1) zone++;
    else return;
    e.preventDefault();
  }

  /** Shows keygroup `i`; `alone` (⌥↑/⌥↓) also makes it the only one selected. */
  function selectKeygroup(i: number, alone = true) {
    if (alone) {
      chosen.clear();
      chosen.add(i);
    }
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
    const targets = info.scope !== "program" && chosen.size > 1 ? [...chosen].sort((a, b) => a - b) : [kg];
    // Show the new value at once.
    for (const k of targets) {
      const g = view.keygroups[k];
      if (info.scope === "zone" && g?.zones[zone] && ref in g.zones[zone].values) g.zones[zone].values[ref] = value;
      else if (info.scope === "program" && ref in view.values) view.values[ref] = value;
      else if (info.scope === "keygroup" && g && ref in g.values) g.values[ref] = value;
      else if (k === kg) live[info.scope === "zone" ? `${ref}#${zone + 1}` : ref] = value;
    }
    for (const k of targets) {
      try {
        await api.writeParam(source.memory, id, info.scope === "program" ? null : k, selector, value);
      } catch (e) {
        const where = targets.length > 1 ? `Keygroup ${k + 1} ` : "";
        app.fail(`${where}${info.label}: ${message(e)}`);
        return;
      }
    }
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
  // Under the keyboard, a bar for each selected keygroup's span, stacked in
  // a strip of fixed height (thinner as more are selected), so selecting
  // never changes the header's size.
  const LANES_TOP = 6.6;
  const LANES = 3.4;
  const lanes = $derived.by(() => {
    const picked = [...chosen].filter((i) => i < ranges.length).sort((a, b) => a - b);
    const step = LANES / Math.max(picked.length, 2);
    return picked.map((i, k) => ({ i, y: LANES_TOP + k * step, h: step * (picked.length > 6 ? 1 : 0.8) }));
  });
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
  <section class="panel" class:env={p.env} style:--route="var(--mta-{p.route})" use:rove={{ tab: "each" }}>
    <h3 class="sign">{p.title}</h3>
    {#if p.env}
      <EnvGraph kind={p.env} get={valueOf} {catalog} onchange={editable ? write : undefined} />
    {/if}
    <div class="cells">
      {#each p.cells as c}{@render cell(c)}{/each}
    </div>
  </section>
{/snippet}

<svelte:window onkeydown={editorKey} />

<div class="editor">
  <div class="bar" data-pane>
    <label>
      Program in memory
      <select bind:value={picked} onchange={() => loadFromMemory(picked)} disabled={!app.connected || loading}>
        <option value="">—</option>
        {#each app.memory.programs as p}<option value={p.name}>{p.name}</option>{/each}
      </select>
    </label>
    <button onclick={loadFile}>Open .akp file…</button>
    {#if view}
      <span class="name">{view.name}</span>
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
    <div class="panes">
      <div class="pane program-pane" data-pane>
        <div class="pane-head"><h2>Program</h2></div>
        <div class="flow">
          {#each programPanels as p}{@render panel(p)}{/each}
          {#if other.length}
            <section class="panel other" style:--route="var(--mta-grey)" use:rove={{ tab: "each" }}>
              <h3 class="sign">Other</h3>
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
        </div>
      </div>

      <div class="pane keygroup-pane" data-pane>
        <!-- The keygroup's tabs, range and the keyboard stay in view as the pane scrolls. -->
        <div class="pane-head">
          <h2>Keygroup</h2>
          <div class="tabs" use:rove>
            {#each view.keygroups as _, i}
              <button class:active={i === kg} class:chosen={chosen.has(i)} aria-pressed={chosen.has(i)} onclick={(e) => pickKeygroup(e, i)}
                >{i + 1}</button
              >
            {/each}
          </div>
          {#if editable && view.keygroups.length > 1}
            <!-- Always there, so the tabs don't re-wrap as the selection changes. -->
            <span class="muted range-text">{chosen.size > 1 && !allKeygroups ? `${chosen.size} selected` : ""}</span>
            <label class="all-toggle" title="Keygroup and zone edits go to every keygroup (zones: the same zone number in each). {modLabel}-click or Shift-click the tabs to pick some.">
              <input type="checkbox" checked={allKeygroups} onchange={(e) => setAll(e.currentTarget.checked)} /> Edit all
            </label>
          {/if}
          {#if keygroup}
            <span class="muted range-text">{noteName(ranges[kg][0])} – {noteName(ranges[kg][1])}</span>
          {/if}
          <svg viewBox="0 0 {whites.length} {LANES_TOP + LANES}" preserveAspectRatio="none" class="keyboard" aria-label="Keygroup ranges">
            {#each whites as n}<rect x={keyX(n)[0]} y="0" width="1" height="6" class="white" />{/each}
            {#each whites as n}
              {#if isBlack(n + 1) && n + 1 <= HIGH}<rect x={keyX(n + 1)[0]} y="0" width="0.6" height="3.8" class="black" />{/if}
            {/each}
            {#each lanes as { i, y, h }}
              {@const [lo, hi] = ranges[i]}
              <rect
                x={keyX(lo)[0]}
                {y}
                width={keyX(hi)[0] + keyX(hi)[1] - keyX(lo)[0]}
                height={h}
                class="range"
                class:current={i === kg}
                role="button"
                tabindex="-1"
                onclick={(e) => pickKeygroup(e, i)}
                onkeydown={() => {}}
              />
            {/each}
          </svg>
        </div>
        <div class="columns">
          {#each keygroupPanels as p}{@render panel(p)}{/each}
          <section class="panel zone" style:--route="var(--mta-lime)" use:rove={{ tab: "each" }}>
            <h3 class="sign">Zone</h3>
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
      </div>
    </div>
  {/if}
</div>

<style>
  /* Two panes, as in ak.Sys: the program's settings on the left, the
     keygroup's and zone's on the right. Each scrolls on its own; panels are
     as wide as their controls and pack together. */
  .editor {
    height: 100%;
    display: flex;
    flex-direction: column;
    padding: 8px 16px 0;
    box-sizing: border-box;
  }
  .bar {
    flex: none;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 12px;
    padding-bottom: 8px;
    border-bottom: 1px solid var(--rule);
  }
  .name {
    font-size: 17px;
    font-weight: 700;
    margin-left: 6px;
  }
  .empty {
    padding: 30px 0;
  }
  .panes {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  /* The panes scroll, which clips anything drawn outside them: room for
     the focus ring (2px, 2px out) round controls at a pane's edge, taken
     from the editor's padding so nothing moves. */
  .pane {
    overflow: auto;
    padding-bottom: 12px;
  }
  .program-pane {
    width: 493px;
    flex: none;
    margin-left: -5px;
    padding-left: 5px;
    padding-right: 14px;
    border-right: 1px solid var(--rule);
  }
  .keygroup-pane {
    flex: 1;
    min-width: 0;
    margin-right: -5px;
    padding-left: 14px;
    padding-right: 5px;
  }
  .pane-head {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 0;
    background: var(--bg);
  }
  .pane-head h2 {
    margin: 0;
    font-size: 14px;
  }
  .flow {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: 8px 14px;
  }
  .flow .panel {
    flex: 1 1 auto;
  }
  .columns {
    column-width: 300px;
    column-gap: 14px;
  }
  .columns .panel {
    break-inside: avoid;
    margin-bottom: 8px;
  }
  .panel {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--rule-soft);
  }
  .panel .sign {
    margin: 0;
    /* A route-coloured edge identifies the panel's function. */
    border-left: 6px solid var(--route);
  }
  .cells {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .stack {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
  }
  .keyboard {
    flex: 1;
    min-width: 120px;
    height: 30px;
    display: block;
  }
  .keyboard .white {
    fill: #fff;
    stroke: #000;
    stroke-width: 0.05;
  }
  .keyboard .black {
    fill: #000;
  }
  /* The selected keygroups' spans; the current one solid. */
  .keyboard .range {
    fill: var(--accent);
    opacity: 0.45;
    stroke: none;
    cursor: pointer;
  }
  .keyboard .range.current {
    opacity: 1;
  }
  .tabs,
  .zone-tabs {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
  }
  .tabs button,
  .zone-tabs button {
    padding: 1px 7px;
    font-size: 11px;
    min-width: 28px;
    margin-right: -1px;
  }
  .tabs button.active,
  .zone-tabs button.active {
    background: var(--text);
    border-color: var(--text);
    color: var(--bg);
  }
  /* Other selected keygroups, which edits also go to. */
  .tabs button.chosen:not(.active) {
    background: var(--select-soft);
  }
  .all-toggle {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    font-weight: 700;
    white-space: nowrap;
  }
  .empty-zone {
    color: var(--muted);
  }
  /* A fixed width (the longest is "C#-1 – G#8"), so the keyboard beside it
     keeps its size from one keygroup to the next. */
  .range-text {
    font-size: 12px;
    white-space: nowrap;
    flex: none;
    width: 11ch;
    font-variant-numeric: tabular-nums;
  }
  .sample {
    margin-left: 8px;
    flex: 1;
    min-width: 100px;
  }
  /* Narrow windows: one pane above the other, scrolling together. */
  @media (max-width: 860px) {
    .panes {
      flex-direction: column;
      overflow: auto;
      margin: 0 -5px;
      padding: 0 5px;
    }
    .pane {
      overflow: visible;
    }
    .program-pane {
      width: auto;
      margin-left: 0;
      padding: 0;
      border-right: none;
    }
    .keygroup-pane {
      margin-right: 0;
      padding: 0;
    }
  }
</style>
