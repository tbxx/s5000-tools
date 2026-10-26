<script lang="ts">
  // Multi editor: the multi's own settings and a parts table like the
  // sampler's Multi page (one row per part, compact inline controls), with
  // the selected part's settings as knobs alongside. Labels, ranges and value
  // names come from spec/params.toml; values come from the multi's .akm, plus
  // a live read of what the file lacks (mute, solo, multi number). Multis in
  // the sampler's memory are edited live; an .akm opened from the Mac is
  // read-only. The FX send bus settings belong to the effects editor.
  import { onMount, untrack } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import Control from "./Control.svelte";
  import NumCell from "./NumCell.svelte";
  import { api, message, type MultiLive, type MultiView, type ParamInfo } from "./api";
  import { dialogs } from "./dialogs.svelte";
  import { noteName } from "./format";
  import { formatPart } from "./partFormat";
  import { app } from "./state.svelte";

  type Route = "grey" | "red" | "brown" | "blue" | "lime";
  interface Col {
    id: string;
    head: string;
    kind: "num" | "enum" | "bool";
    bar?: boolean;
  }
  interface Group {
    title: string;
    route: Route;
    cols: Col[];
  }

  const P = "multi_part.";
  // Column groups in the order of the sampler's Multi page; each group's
  // route colour matches its panel in the part detail. The part itself is
  // purple, the view's own bullet.
  const groups: Group[] = [
    {
      title: "MIDI", route: "grey",
      cols: [{ id: `${P}midi_channel`, head: "Ch", kind: "enum" }],
    },
    {
      title: "Mix", route: "red",
      cols: [
        { id: `${P}level`, head: "Level", kind: "num", bar: true },
        { id: `${P}pan`, head: "Pan", kind: "num", bar: true },
        { id: `${P}output`, head: "Out", kind: "enum" },
        { id: `${P}mute`, head: "M", kind: "bool" },
        { id: `${P}solo`, head: "S", kind: "bool" },
      ],
    },
    {
      title: "FX", route: "brown",
      cols: [
        { id: `${P}fx_channel`, head: "Bus", kind: "enum" },
        { id: `${P}fx_send`, head: "Send", kind: "num", bar: true },
      ],
    },
    {
      title: "Tune", route: "blue",
      cols: [
        { id: `${P}semitone`, head: "Semi", kind: "num", bar: true },
        { id: `${P}fine`, head: "Fine", kind: "num", bar: true },
      ],
    },
    {
      title: "Keys", route: "lime",
      cols: [
        { id: `${P}low_key`, head: "Low", kind: "num" },
        { id: `${P}high_key`, head: "High", kind: "num" },
      ],
    },
  ];

  /** The part detail: the same settings as knobs, grouped as in the table. */
  const detail: { title: string; route: Route; ids: string[] }[] = [
    { title: "MIDI", route: "grey", ids: ["midi_channel", "mute", "solo"] },
    { title: "Mix", route: "red", ids: ["level", "pan", "output"] },
    { title: "FX send", route: "brown", ids: ["fx_channel", "fx_send"] },
    { title: "Tune", route: "blue", ids: ["semitone", "fine"] },
    { title: "Key range", route: "lime", ids: ["low_key", "high_key"] },
  ];
  /** Mute and solo light up as on a mixer. */
  const lit: Record<string, string> = { [`${P}mute`]: "var(--mta-red)", [`${P}solo`]: "var(--mta-green)" };

  const LOW = 21;
  const HIGH = 127;
  const emptyLive = (): MultiLive => ({ values: {}, parts: [] });

  let catalog = $state<Map<string, ParamInfo>>(new Map());
  let view = $state<MultiView | null>(null);
  /** Where the multi came from: a name in a sampler's memory, or a file. */
  let source = $state<{ memory: string; sampler: number } | { file: string } | null>(null);
  let live = $state<MultiLive>(emptyLive());
  let sel = $state(0);
  let picked = $state("");
  let loading = $state(false);
  let usedOnly = $state(false);
  /** Parts in new multis: 0 = 32, 1 = 64, 2 = 128. */
  let newParts = $state(0);

  const editable = $derived(
    !!source && "memory" in source && app.connected && app.active?.id === source.sampler,
  );
  const part = $derived(view?.parts[sel]);
  const rows = $derived(
    (view?.parts ?? []).map((p, i) => ({ p, i })).filter(({ p, i }) => !usedOnly || p.program || i === sel),
  );
  const used = $derived(view?.parts.filter((p) => p.program).length ?? 0);
  const info = (id: string) => catalog.get(id);
  const newPartOptions = $derived(info("multi.new_part_count")?.options ?? ["32", "64", "128"]);

  onMount(async () => {
    try {
      catalog = new Map((await api.multiCatalog()).map((p) => [p.id, p]));
    } catch (e) {
      app.fail(e);
    }
  });

  // Follow the sampler: a switch to another sampler reloads the multi of the
  // same name there (or clears the editor), and a change to the memory list
  // (a transfer, rename or delete) re-reads it.
  let seen = app.changed;
  $effect(() => {
    const id = app.active?.id;
    const connected = app.connected;
    const changed = app.changed;
    untrack(() => {
      if (!source || !("memory" in source) || !connected) return;
      const name = source.memory;
      const there = app.memory.multis.some((m) => m.name === name);
      if (id !== source.sampler || changed !== seen) {
        seen = changed;
        if (there) void loadFromMemory(name, id !== source.sampler);
        else clear();
      }
    });
  });

  function clear() {
    view = null;
    source = null;
    live = emptyLive();
    picked = "";
    sel = 0;
  }

  async function loadFromMemory(name: string, fresh = true) {
    const sampler = app.active?.id;
    if (!name || sampler === undefined) return;
    loading = true;
    try {
      const v = await api.readMulti(name);
      view = v;
      source = { memory: name, sampler };
      picked = name;
      if (fresh || sel >= v.parts.length) sel = 0;
      await readLive();
    } catch (e) {
      app.fail(e);
    } finally {
      loading = false;
    }
  }

  async function loadFile() {
    const path = await open({ filters: [{ name: "Multis", extensions: ["akm", "AKM"] }], defaultPath: app.settings.folder });
    if (typeof path !== "string") return;
    try {
      view = await api.readMultiFile(path);
      source = { file: path };
      live = emptyLive();
      sel = 0;
      picked = "";
    } catch (e) {
      app.fail(e);
    }
  }

  async function readLive() {
    live = emptyLive();
    if (!source || !("memory" in source) || !app.connected) return;
    try {
      live = await api.readMultiLive(source.memory);
    } catch (e) {
      app.fail(`Live values: ${message(e)}`);
    }
  }

  async function newMulti() {
    const r = await dialogs.ask(
      "New multi",
      [
        { label: "Name", value: `Multi ${app.memory.multis.length + 1}` },
        { label: info("multi.new_part_count")?.label ?? "Parts", value: newPartOptions[newParts], options: newPartOptions },
      ],
      "Create",
    );
    if (!r || !r[0].trim()) return;
    newParts = Math.max(0, newPartOptions.indexOf(r[1]));
    try {
      await api.createMulti(undefined, r[0].trim(), parseInt(r[1]));
      await app.refresh();
      seen = app.changed;
      await loadFromMemory(r[0].trim());
    } catch (e) {
      app.fail(e);
    }
  }

  async function rename() {
    if (!view || !source || !("memory" in source)) return;
    const old = source.memory;
    const r = await dialogs.ask("Rename multi", [{ label: "Name", value: old }], "Rename");
    const name = r?.[0].trim();
    if (!name || name === old) return;
    try {
      await api.rename(undefined, "multi", old, name);
      await app.refresh();
      seen = app.changed;
      await loadFromMemory(name, false);
    } catch (e) {
      app.fail(e);
    }
  }

  /** A multi value (`part` null) or a part's, from the file or the live read. */
  function valueOf(p: number | null, id: string): number | undefined {
    if (!view) return undefined;
    if (p === null) return view.values[id] ?? live.values[id];
    return view.parts[p]?.values[id] ?? live.parts[p]?.[id];
  }

  async function write(p: number | null, id: string, value: number) {
    if (!editable || !view || !source || !("memory" in source)) return;
    // Show the new value at once.
    if (p === null) {
      if (id in view.values) view.values[id] = value;
      else live.values[id] = value;
    } else if (id in view.parts[p].values) view.parts[p].values[id] = value;
    else (live.parts[p] ??= {})[id] = value;
    try {
      await api.writeMultiParam(source.memory, id, p, value);
    } catch (e) {
      const where = p === null ? "" : `Part ${p + 1} `;
      app.fail(`${where}${info(id)?.label ?? id}: ${message(e)}`);
    }
  }

  async function assign(p: number, program: string) {
    if (!editable || !view || !source || !("memory" in source) || !program) return;
    const before = view.parts[p].program;
    view.parts[p].program = program;
    try {
      await api.setPartProgram(source.memory, p, program);
    } catch (e) {
      view.parts[p].program = before;
      app.fail(`Part ${p + 1} program: ${message(e)}`);
    }
  }

  /** Programs to offer for a part: memory, plus the part's own if it is not loaded. */
  function programChoices(current: string | null): string[] {
    const names = app.memory.programs.map((x) => x.name);
    return current && !names.includes(current) ? [current, ...names] : names;
  }

  /** Left edge and width of a key range, in % of A-1 … G8. */
  function span(p: number): [number, number] | null {
    const lo = valueOf(p, `${P}low_key`);
    const hi = valueOf(p, `${P}high_key`);
    if (lo === undefined || hi === undefined || hi < lo) return null;
    const x = (n: number) => ((Math.min(HIGH, Math.max(LOW, n)) - LOW) / (HIGH - LOW + 1)) * 100;
    return [x(lo), x(hi + 1) - x(lo)];
  }

  /** "FX send" in the FX send panel is "Send". */
  function shortDetailLabel(i: ParamInfo) {
    const s = i.label.replace(/^FX /, "");
    return s.charAt(0).toUpperCase() + s.slice(1);
  }
</script>

{#snippet programPicker(p: number, current: string | null, wide: boolean)}
  {#if editable}
    <select
      class="cell-select program"
      class:wide
      value={current ?? ""}
      onchange={(e) => assign(p, e.currentTarget.value)}
      aria-label="Part {p + 1} program"
    >
      {#if !current}<option value="" disabled>—</option>{/if}
      {#each programChoices(current) as name}<option value={name}>{name}</option>{/each}
    </select>
  {:else}
    <span class="program-name" class:none={!current}>{current ?? "—"}</span>
  {/if}
{/snippet}

{#snippet cellFor(p: number, c: Col)}
  {@const i = info(c.id)}
  {@const v = valueOf(p, c.id)}
  {#if i}
    {#if c.kind === "num"}
      <NumCell info={i} value={v} bar={c.bar} onchange={editable ? (x) => write(p, c.id, x) : undefined} />
    {:else if c.kind === "enum"}
      {#if editable && v !== undefined}
        <select class="cell-select" value={v} onchange={(e) => write(p, c.id, parseInt(e.currentTarget.value))} aria-label="Part {p + 1} {i.label}">
          {#each i.options ?? [] as o, k}<option value={k}>{o}</option>{/each}
        </select>
      {:else}
        <span class="text" class:unknown={v === undefined}>{v === undefined ? "—" : formatPart(i, v)}</span>
      {/if}
    {:else}
      <button
        class="tog"
        class:on={v === 1}
        style:--lit={lit[c.id]}
        disabled={!editable || v === undefined}
        aria-pressed={v === 1}
        aria-label="Part {p + 1} {i.label}"
        title={v === undefined ? `${i.label}: not stored in the file` : i.label}
        onclick={() => write(p, c.id, v === 1 ? 0 : 1)}
      >{v === undefined ? "·" : c.head}</button>
    {/if}
  {/if}
{/snippet}

<div class="editor">
  <div class="bar">
    <label>
      Multi in memory
      <select bind:value={picked} onchange={() => loadFromMemory(picked)} disabled={!app.connected || loading}>
        <option value="">—</option>
        {#each app.memory.multis as m}<option value={m.name}>{m.name}</option>{/each}
      </select>
    </label>
    <button onclick={newMulti} disabled={!app.connected || loading}>New multi…</button>
    <button onclick={loadFile}>Open .akm file…</button>
    {#if view}
      <span class="name">{view.name}</span>
      <span class="muted">
        {view.parts.length} parts, {used} with a program ·
        {editable ? "editing the sampler's memory live" : source && "file" in source ? "read-only (file)" : "read-only (not connected)"}
        {#if editable && app.info?.simulated}· the simulator does not take parameter edits{/if}
      </span>
    {/if}
  </div>

  {#if !view}
    <p class="empty muted">
      Choose a multi in the sampler's memory, make a new one, or open a saved .akm file. Mute, solo and the multi number
      are not stored in the file{app.connected ? "" : "; connect to read them live"}.
    </p>
  {:else}
    <section class="panel multi" style:--route="var(--mta-grey)">
      <h3 class="sign">Multi</h3>
      <div class="cells">
        <div class="field">
          <span class="label">{info("multi.name")?.label ?? "Name"}</span>
          <span class="value-row">
            <span class="lcd big">{view.name}</span>
            {#if editable}<button class="small" onclick={rename}>Rename…</button>{/if}
          </span>
        </div>
        {#if info("multi.program_number")}
          <Control
            info={info("multi.program_number")!}
            value={valueOf(null, "multi.program_number")}
            onchange={editable ? (v) => write(null, "multi.program_number", v) : undefined}
          />
        {/if}
        <div class="field">
          <span class="label">{info("multi.part_count")?.label ?? "Parts"}</span>
          <span class="lcd">{valueOf(null, "multi.part_count") ?? "—"}</span>
        </div>
        <div class="field" title={info("multi.new_part_count")?.notes ?? ""}>
          <span class="label">{info("multi.new_part_count")?.label ?? "Parts in new multis"}</span>
          <span class="segments">
            {#each newPartOptions as o, k}
              <button class="small" class:active={k === newParts} onclick={() => (newParts = k)}>{o}</button>
            {/each}
          </span>
        </div>
        <label class="used">
          <input type="checkbox" bind:checked={usedOnly} />
          Only parts with a program
        </label>
      </div>
    </section>

    <div class="layout">
      <div class="table-wrap">
        <table class="parts">
          <colgroup>
            <col class="w-part" />
            <col class="w-program" />
            {#each groups as g}
              {#each g.cols as c}<col class="w-{c.kind === 'bool' ? 'bool' : c.kind === 'enum' ? 'enum' : 'num'}" />{/each}
            {/each}
            <col class="w-range" />
          </colgroup>
          <thead>
            <tr class="groups">
              <th colspan="2" class="sign-cell" style:--route="var(--mta-purple)">Part</th>
              {#each groups as g}
                <th colspan={g.cols.length + (g.title === "Keys" ? 1 : 0)} class="sign-cell" style:--route="var(--mta-{g.route})">
                  {g.title}
                </th>
              {/each}
            </tr>
            <tr class="heads">
              <th class="num-head">#</th>
              <th>Program</th>
              {#each groups as g}
                {#each g.cols as c}<th class:center={c.kind !== "num"} class:num-head={c.kind === "num"} title={info(c.id)?.label}>{c.head}</th>{/each}
              {/each}
              <th>Range</th>
            </tr>
          </thead>
          <tbody>
            {#each rows as { p, i } (i)}
              <tr class:selected={i === sel} class:empty-part={!p.program} onpointerdown={() => (sel = i)}>
                <td class="partno"><button class="pick" onclick={() => (sel = i)} aria-label="Part {i + 1}">{i + 1}</button></td>
                <td>{@render programPicker(i, p.program, false)}</td>
                {#each groups as g}
                  {#each g.cols as c}<td class:center={c.kind !== "num"} style:--route="var(--mta-{g.route})">{@render cellFor(i, c)}</td>{/each}
                {/each}
                <td class="range" title={span(i) ? `${noteName(valueOf(i, `${P}low_key`)!)} – ${noteName(valueOf(i, `${P}high_key`)!)}` : ""}>
                  {#if span(i)}
                    {@const [x, w] = span(i)!}
                    <span class="track"><span class="span" style:left="{x}%" style:width="{w}%"></span></span>
                  {/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
        {#if !rows.length}<p class="muted no-rows">No part has a program.</p>{/if}
      </div>

      {#if part}
        <aside class="detail">
          <h3 class="sign head" style:--route="var(--mta-purple)">
            Part {sel + 1}
            <span class="sign-muted">{part.program ?? "no program"}</span>
            <span class="step">
              <button class="small" onclick={() => (sel = Math.max(0, sel - 1))} disabled={sel === 0} aria-label="Previous part">‹</button>
              <button class="small" onclick={() => (sel = Math.min(view!.parts.length - 1, sel + 1))} disabled={sel === view.parts.length - 1} aria-label="Next part">›</button>
            </span>
          </h3>
          <div class="program-row">
            <span class="label">Program</span>
            {@render programPicker(sel, part.program, true)}
          </div>
          {#each detail as d}
            <section class="panel" style:--route="var(--mta-{d.route})">
              <h3 class="sign">{d.title}</h3>
              <div class="cells">
                {#each d.ids as id}
                  {@const i = info(P + id)}
                  {#if i}
                    <span class="ctl" style:--route={lit[P + id] ?? `var(--mta-${d.route})`}>
                      <Control
                        info={i}
                        value={valueOf(sel, P + id)}
                        label={shortDetailLabel(i)}
                        onchange={editable ? (v) => write(sel, P + id, v) : undefined}
                      />
                    </span>
                  {/if}
                {/each}
              </div>
            </section>
          {/each}
        </aside>
      {/if}
    </div>
  {/if}
</div>

<style>
  .editor {
    padding: 12px 16px 20px;
  }
  .bar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 12px;
    padding-bottom: 10px;
    margin-bottom: 12px;
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
  button.small {
    padding: 0 6px;
    font-size: 11px;
  }
  .panel {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-bottom: 10px;
    border-bottom: 1px solid var(--rule-soft);
  }
  .panel .sign {
    margin: 0;
    border-left: 6px solid var(--route);
  }
  .cells {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 8px 14px;
    padding: 0 4px;
  }
  .panel.multi {
    margin-bottom: 16px;
  }
  .panel.multi .cells {
    gap: 8px 24px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  /* As Control's labels. */
  .label {
    font-size: 9.5px;
    font-weight: 700;
    letter-spacing: 0.01em;
  }
  .value-row {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .lcd {
    border-color: var(--rule);
    min-width: 42px;
    text-align: center;
  }
  .lcd.big {
    font-size: 12px;
    font-weight: 700;
    min-width: 150px;
    text-align: left;
  }
  .segments {
    display: flex;
  }
  .segments button {
    margin-right: -1px;
    min-width: 34px;
  }
  .segments button.active {
    background: var(--text);
    border-color: var(--text);
    color: var(--bg);
  }
  .used {
    margin-left: auto;
    font-size: 12px;
    align-self: center;
  }

  .layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 300px;
    gap: 16px;
    align-items: start;
  }
  @media (max-width: 1100px) {
    .layout {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .table-wrap {
    min-width: 0;
    overflow-x: auto;
  }

  /* Parts table: 22 px rows with soft hairlines; the group row is signage. */
  .parts {
    width: 100%;
    min-width: 780px;
    table-layout: fixed;
    border-collapse: collapse;
    font-size: 11px;
  }
  .w-part {
    width: 30px;
  }
  .w-program {
    width: 22%;
  }
  .w-enum {
    width: 52px;
  }
  .w-num {
    width: 46px;
  }
  .w-bool {
    width: 24px;
  }
  .w-range {
    width: 72px;
  }
  thead th {
    position: sticky;
    background: var(--surface);
    z-index: 1;
  }
  .groups th {
    top: 0;
  }
  .heads th {
    top: 22px;
  }
  .sign-cell {
    height: 22px;
    padding: 2px 6px;
    background: var(--sign-bg);
    color: var(--sign-text);
    font-size: 11px;
    font-weight: 700;
    text-align: left;
    border-left: 4px solid var(--route);
  }
  .heads th {
    height: 20px;
    padding: 2px 4px;
    font-size: 9.5px;
    font-weight: 700;
    text-align: left;
    color: var(--text);
    border-bottom: 1px solid var(--rule);
  }
  th.num-head {
    text-align: right;
    padding-right: 6px;
  }
  .center {
    text-align: center;
  }
  td {
    height: 22px;
    padding: 1px 2px;
    border-bottom: 1px solid var(--rule-soft);
    vertical-align: middle;
    white-space: nowrap;
    overflow: hidden;
  }
  tr.selected td {
    background: var(--select-soft);
  }
  tr.selected td.partno {
    background: var(--select);
  }
  tr.selected .pick {
    color: var(--select-text);
  }
  /* Parts without a program recede, so the ones in use stand out. */
  tr.empty-part:not(.selected) td:not(.partno) {
    opacity: 0.5;
  }
  .empty-part .program-name,
  .empty-part .pick {
    color: var(--muted);
  }
  .pick {
    border: none;
    background: none;
    padding: 0 4px 0 0;
    width: 100%;
    text-align: right;
    font-size: 11px;
  }
  .pick:hover:not(:disabled) {
    background: none;
    text-decoration: underline;
  }
  .program-name {
    display: block;
    padding: 0 4px;
    overflow: hidden;
    text-overflow: ellipsis;
    font-weight: 700;
  }
  .program-name.none {
    font-weight: 400;
  }
  .cell-select {
    width: 100%;
    font-size: 11px;
    padding: 0 1px;
    height: 18px;
    border-color: var(--rule-soft);
  }
  .cell-select:hover {
    border-color: var(--rule);
  }
  .cell-select.program {
    font-weight: 700;
  }
  .cell-select.wide {
    width: auto;
    min-width: 180px;
    font-size: 12px;
    height: 22px;
  }
  .text {
    display: block;
    padding: 0 3px;
  }
  .unknown {
    color: var(--muted);
  }
  .tog {
    width: 20px;
    height: 18px;
    padding: 0;
    font-size: 10px;
    color: var(--muted);
    border-color: var(--rule-soft);
  }
  .tog:hover:not(:disabled) {
    color: var(--text);
    border-color: var(--rule);
  }
  .tog.on,
  .tog.on:hover:not(:disabled) {
    background: var(--lit);
    border-color: var(--lit);
    color: #fff;
  }
  .tog:disabled {
    opacity: 1;
    color: var(--muted);
  }
  .tog.on:disabled {
    color: #fff;
  }
  .range {
    padding: 0 4px;
  }
  .track {
    position: relative;
    display: block;
    height: 6px;
    background: var(--surface-2);
    border: 1px solid var(--rule-soft);
  }
  .span {
    position: absolute;
    top: 0;
    bottom: 0;
    background: var(--mta-lime);
  }
  .no-rows {
    padding: 12px 4px;
  }

  /* Part detail: the selected part as knobs, beside the table. */
  .detail {
    position: sticky;
    top: 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .detail .head {
    margin: 0;
    border-left: 6px solid var(--route);
    font-size: 13px;
    min-height: 26px;
  }
  .sign-muted {
    color: var(--sign-muted);
    font-weight: 400;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .step {
    margin-left: auto;
    display: flex;
  }
  .step button {
    background: transparent;
    color: var(--sign-text);
    border-color: var(--sign-muted);
    margin-left: -1px;
    padding: 0 7px;
  }
  .step button:hover:not(:disabled) {
    background: transparent;
    border-color: var(--sign-text);
  }
  .program-row {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 0 4px;
  }
  .program-row .program-name {
    padding: 0;
    font-size: 13px;
  }
  .detail .cells {
    justify-content: flex-start;
    gap: 8px 6px;
  }
  .ctl {
    display: contents;
  }
</style>
