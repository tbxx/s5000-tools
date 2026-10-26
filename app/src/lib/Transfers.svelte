<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, formatBytes, message, type Entry, type Kind } from "./api";
  import { dialogs } from "./dialogs.svelte";
  import { queue } from "./queue.svelte";
  import { app } from "./state.svelte";

  type Side = "sampler" | "mac";
  interface Row {
    key: string;
    kind: Kind;
    name: string;
    detail: string;
    size: number | null;
  }

  // --- Sampler side -------------------------------------------------------------
  const groups = $derived<{ kind: Kind; title: string; rows: Row[] }[]>([
    {
      kind: "sample",
      title: "Samples",
      rows: app.memory.samples.map((s) => ({
        key: `sample:${s.name}`,
        kind: "sample",
        name: s.name,
        detail: `${(s.sample_rate / 1000).toFixed(1)} kHz ${s.channels === 2 ? "stereo" : s.channels === 1 ? "mono" : `${s.channels} ch`} · ${(s.frames / (s.sample_rate || 1)).toFixed(2)} s`,
        size: s.bytes,
      })),
    },
    {
      kind: "program",
      title: "Programs",
      rows: app.memory.programs.map((p) => ({
        key: `program:${p.name}`,
        kind: "program",
        name: p.name,
        detail: `${p.count} keygroup${p.count === 1 ? "" : "s"}`,
        size: null,
      })),
    },
    {
      kind: "multi",
      title: "Multis",
      rows: app.memory.multis.map((m) => ({
        key: `multi:${m.name}`,
        kind: "multi",
        name: m.name,
        detail: `${m.count} parts`,
        size: null,
      })),
    },
  ]);
  const samplerRows = $derived(groups.flatMap((g) => g.rows));
  let samplerSel = $state(new Set<string>());
  let samplerAnchor = $state<string | null>(null);
  let playing = $state<string | null>(null);
  const selectedItems = $derived(samplerRows.filter((r) => samplerSel.has(r.key)));

  // --- Mac side --------------------------------------------------------------------
  let entries = $state<Entry[]>([]);
  let macSel = $state(new Set<string>());
  let macAnchor = $state<string | null>(null);
  const folder = $derived(app.settings.folder ?? "");
  const selectedFiles = $derived(entries.filter((e) => macSel.has(e.path) && e.kind !== "dir"));

  async function loadFolder() {
    if (!folder) return;
    try {
      entries = await api.listFolder(folder);
      macSel = new Set([...macSel].filter((p) => entries.some((e) => e.path === p)));
    } catch (e) {
      entries = [];
      app.fail(e);
    }
  }

  // Reload when the folder changes or a batch of transfers finishes.
  $effect(() => {
    void queue.finished;
    void folder;
    untrack(loadFolder);
  });

  async function chooseFolder() {
    const picked = await open({ directory: true, defaultPath: folder || undefined });
    if (typeof picked === "string") setFolder(picked);
  }

  function setFolder(path: string) {
    macSel = new Set();
    app.save({ folder: path });
  }

  function up() {
    const parent = folder.replace(/\/[^/]+\/?$/, "") || "/";
    setFolder(parent);
  }

  // --- Selection -------------------------------------------------------------------
  function select(
    e: MouseEvent,
    key: string,
    order: string[],
    sel: Set<string>,
    anchor: string | null,
  ): [Set<string>, string | null] {
    if (e.shiftKey && anchor) {
      const [a, b] = [order.indexOf(anchor), order.indexOf(key)].sort((x, y) => x - y);
      return [new Set(order.slice(a, b + 1)), anchor];
    }
    if (e.metaKey || e.ctrlKey) {
      const next = new Set(sel);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return [next, key];
    }
    return [new Set([key]), key];
  }

  function clickSampler(e: MouseEvent, row: Row) {
    [samplerSel, samplerAnchor] = select(e, row.key, samplerRows.map((r) => r.key), samplerSel, samplerAnchor);
  }

  function clickMac(e: MouseEvent, entry: Entry) {
    [macSel, macAnchor] = select(e, entry.path, entries.map((x) => x.path), macSel, macAnchor);
  }

  // --- Actions ---------------------------------------------------------------------
  function getSelected(rows = selectedItems) {
    if (!folder) {
      app.fail("Choose a folder on the Mac first.");
      return;
    }
    for (const r of rows) {
      queue.add({
        direction: "get",
        label: `${r.name}.${r.kind === "sample" ? "wav" : r.kind === "program" ? "akp" : "akm"}`,
        kind: r.kind,
        source: r.name,
        folder,
        deps: !!app.settings.deps && r.kind !== "sample",
        replace: !!app.settings.replace,
        size: r.size === null ? null : r.size + 44,
      });
    }
  }

  function sendFiles(files: { path: string; name: string; kind: Kind; size: number | null }[]) {
    if (!app.connected) {
      app.fail("Connect to a sampler first.");
      return;
    }
    for (const f of files) {
      queue.add({
        direction: "put",
        label: f.name,
        kind: f.kind,
        source: f.path,
        folder: "",
        deps: !!app.settings.deps && f.kind !== "sample",
        replace: false,
        size: f.size,
      });
    }
  }

  const sendSelected = () =>
    sendFiles(selectedFiles.map((e) => ({ path: e.path, name: e.name, kind: e.kind as Kind, size: e.size })));

  async function play(row: Row) {
    try {
      if (playing) await api.audition(playing, false);
      if (playing === row.name) {
        playing = null;
      } else {
        await api.audition(row.name, true);
        playing = row.name;
      }
    } catch (e) {
      playing = null;
      app.fail(e);
    }
  }

  async function remove() {
    const items = selectedItems;
    if (!items.length) return;
    const names = items.map((r) => r.name);
    const list = names.length > 8 ? `${names.slice(0, 8).join(", ")} and ${names.length - 8} more` : names.join(", ");
    const ok = await dialogs.confirm(
      `Delete ${items.length} item${items.length === 1 ? "" : "s"} from memory?`,
      `${list}\n\nThis removes them from the sampler's memory. It cannot be undone.`,
      "Delete",
      true,
    );
    if (!ok) return;
    try {
      for (const kind of ["multi", "program", "sample"] as Kind[]) {
        const n = items.filter((r) => r.kind === kind).map((r) => r.name);
        if (n.length) await api.deleteItems(kind, n);
      }
      app.say(`Deleted ${list}`);
      samplerSel = new Set();
    } catch (e) {
      app.fail(e);
    }
    await app.refresh();
  }

  async function rename() {
    const [row] = selectedItems;
    if (!row || selectedItems.length !== 1) return;
    const v = await dialogs.ask(`Rename ${row.kind}`, [{ label: "Name", value: row.name }], "Rename");
    if (!v || !v[0].trim() || v[0] === row.name) return;
    const confirmed = await dialogs.confirm(`Rename "${row.name}" to "${v[0]}"?`, "", "Rename");
    if (!confirmed) return;
    try {
      await api.rename(row.kind, row.name, v[0]);
      samplerSel = new Set([`${row.kind}:${v[0]}`]);
      app.say(`Renamed ${row.name} to ${v[0]}`);
    } catch (e) {
      app.fail(e);
    }
    await app.refresh();
  }

  async function newProgram() {
    const v = await dialogs.ask("New program", [
      { label: "Name", value: "New Program" },
      { label: "Keygroups", value: "1" },
    ], "Create");
    if (!v) return;
    const kgs = Math.max(1, Math.min(99, parseInt(v[1]) || 1));
    await api.createProgram(v[0], kgs).catch((e) => app.fail(e));
    await app.refresh();
  }

  async function newMulti() {
    const v = await dialogs.ask("New multi", [
      { label: "Name", value: "New Multi" },
      { label: "Parts", value: "32", options: ["32", "64", "128"] },
    ], "Create");
    if (!v) return;
    await api.createMulti(v[0], parseInt(v[1])).catch((e) => app.fail(e));
    await app.refresh();
  }

  function samplerKey(e: KeyboardEvent) {
    if (e.key === "Backspace" || e.key === "Delete") remove();
    else if (e.key === "Enter") getSelected();
    else if ((e.metaKey || e.ctrlKey) && e.key === "a") {
      e.preventDefault();
      samplerSel = new Set(samplerRows.map((r) => r.key));
    } else if (e.key === " " && selectedItems.length === 1 && selectedItems[0].kind === "sample") {
      e.preventDefault();
      play(selectedItems[0]);
    }
  }

  function macKey(e: KeyboardEvent) {
    if (e.key === "Enter") sendSelected();
    else if ((e.metaKey || e.ctrlKey) && e.key === "a") {
      e.preventDefault();
      macSel = new Set(entries.filter((x) => x.kind !== "dir").map((x) => x.path));
    }
  }

  // --- Dragging between the panes (pointer based, so it works in every webview) ---
  let drag = $state<{ from: Side; label: string; x: number; y: number; over: Side | null } | null>(null);
  let pressed: { from: Side; x: number; y: number } | null = null;

  function sideAt(x: number, y: number): Side | null {
    const el = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-drop]");
    return (el?.dataset.drop as Side) ?? null;
  }

  function pointerDown(e: PointerEvent, from: Side, key: string) {
    if (e.button !== 0 || e.shiftKey || e.metaKey || e.ctrlKey) return;
    if (from === "sampler" && !samplerSel.has(key)) [samplerSel, samplerAnchor] = [new Set([key]), key];
    if (from === "mac" && !macSel.has(key)) [macSel, macAnchor] = [new Set([key]), key];
    pressed = { from, x: e.clientX, y: e.clientY };
  }

  function pointerMove(e: PointerEvent) {
    if (!pressed) return;
    if (!drag) {
      if (Math.hypot(e.clientX - pressed.x, e.clientY - pressed.y) < 5) return;
      const n = pressed.from === "sampler" ? selectedItems.length : selectedFiles.length;
      if (!n) return;
      const first = pressed.from === "sampler" ? selectedItems[0].name : selectedFiles[0].name;
      drag = { from: pressed.from, label: n === 1 ? first : `${n} items`, x: 0, y: 0, over: null };
    }
    drag.x = e.clientX;
    drag.y = e.clientY;
    drag.over = sideAt(e.clientX, e.clientY);
  }

  function pointerUp() {
    if (drag && drag.over && drag.over !== drag.from) {
      if (drag.from === "sampler") getSelected();
      else sendSelected();
    }
    drag = null;
    pressed = null;
  }

  // --- Files dropped from the Finder ------------------------------------------------
  let finderOver = $state<Side | null>(null);

  onMount(() => {
    const unlisten = getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (payload.type === "leave") {
        finderOver = null;
        return;
      }
      const r = window.devicePixelRatio || 1;
      const side = sideAt(payload.position.x / r, payload.position.y / r);
      if (payload.type !== "drop") {
        finderOver = side;
        return;
      }
      finderOver = null;
      if (side === "sampler") {
        const files = payload.paths
          .map((path) => {
            const name = path.split("/").pop() ?? path;
            const ext = name.split(".").pop()?.toLowerCase();
            const kind: Kind | null = ext === "wav" ? "sample" : ext === "akp" ? "program" : ext === "akm" ? "multi" : null;
            return kind && { path, name, kind, size: null };
          })
          .filter((f) => f !== null);
        if (files.length) sendFiles(files);
        else app.fail("Only .wav, .akp and .akm files can be sent to the sampler.");
      } else if (side === "mac" && payload.paths.length === 1) {
        api
          .listFolder(payload.paths[0])
          .then(() => setFolder(payload.paths[0]))
          .catch(() => app.fail("Drop a folder here to open it."));
      }
    });
    return () => {
      unlisten.then((f) => f());
      if (playing) api.audition(playing, false).catch(() => {});
    };
  });

  const kindIcon: Record<string, string> = { dir: "▸", sample: "∿", program: "P", multi: "M" };
  const busy = $derived(queue.running);
</script>

<svelte:window onpointermove={pointerMove} onpointerup={pointerUp} />

<div class="transfers" class:dragging={drag}>
  <section
    class="pane"
    data-drop="sampler"
    class:target={(drag && drag.from === "mac" && drag.over === "sampler") || finderOver === "sampler"}
  >
    <div class="head">
      <span class="title">Sampler memory</span>
      <span class="tools">
        <button onclick={newProgram} disabled={!app.connected}>New program</button>
        <button onclick={newMulti} disabled={!app.connected}>New multi</button>
        <button onclick={rename} disabled={selectedItems.length !== 1 || busy}>Rename</button>
        <button onclick={remove} disabled={!selectedItems.length || busy}>Delete</button>
        <button onclick={() => app.refresh()} disabled={!app.connected} title="Reload the memory list">↻</button>
      </span>
    </div>
    <div class="list" role="listbox" aria-multiselectable="true" tabindex="0" onkeydown={samplerKey}>
      {#if !app.connected}
        <p class="empty">Not connected. Choose Simulator or Real sampler at the top right.</p>
      {:else}
        {#each groups as g}
          <div class="group">{g.title} <span class="muted">{g.rows.length}</span></div>
          {#each g.rows as row (row.key)}
            <!-- svelte-ignore a11y_click_events_have_key_events (the listbox handles keys) -->
            <div
              class="row"
              role="option"
              tabindex="-1"
              aria-selected={samplerSel.has(row.key)}
              class:selected={samplerSel.has(row.key)}
              onclick={(e) => clickSampler(e, row)}
              ondblclick={() => getSelected([row])}
              onpointerdown={(e) => pointerDown(e, "sampler", row.key)}
            >
              <span class="name">{row.name}</span>
              <span class="detail">{row.detail}</span>
              <span class="size">{row.size === null ? "" : formatBytes(row.size)}</span>
              <span class="act">
                {#if row.kind === "sample"}
                  <button
                    class="play"
                    class:on={playing === row.name}
                    title={playing === row.name ? "Stop" : "Play on the sampler"}
                    onclick={(e) => {
                      e.stopPropagation();
                      play(row);
                    }}
                    onpointerdown={(e) => e.stopPropagation()}>{playing === row.name ? "■" : "▶"}</button
                  >
                {/if}
              </span>
            </div>
          {:else}
            <div class="none">none</div>
          {/each}
        {/each}
      {/if}
    </div>
  </section>

  <div class="middle">
    <button onclick={() => getSelected()} disabled={!selectedItems.length || !folder} title="Copy the selected items to the Mac folder">
      Get →
    </button>
    <button onclick={sendSelected} disabled={!selectedFiles.length || !app.connected} title="Send the selected files to the sampler">
      ← Send
    </button>
    <label title="Get a program's samples, or a multi's programs and samples; send them too if they are not in memory">
      <input type="checkbox" checked={app.settings.deps} onchange={(e) => app.save({ deps: e.currentTarget.checked })} />
      Include dependencies
    </label>
    <label title="Overwrite files in the Mac folder that have the same name">
      <input type="checkbox" checked={app.settings.replace} onchange={(e) => app.save({ replace: e.currentTarget.checked })} />
      Replace existing files
    </label>
  </div>

  <section
    class="pane"
    data-drop="mac"
    class:target={(drag && drag.from === "sampler" && drag.over === "mac") || finderOver === "mac"}
  >
    <div class="head">
      <span class="title">Mac folder</span>
      <span class="tools">
        <button onclick={up} disabled={!folder || folder === "/"} title="Enclosing folder">↑</button>
        <button onclick={chooseFolder}>Choose…</button>
        <button onclick={loadFolder} disabled={!folder} title="Reload">↻</button>
      </span>
    </div>
    <div class="path lcd" title={folder}><bdi>{folder || "No folder chosen"}</bdi></div>
    <div class="list" role="listbox" aria-multiselectable="true" tabindex="0" onkeydown={macKey}>
      {#if !folder}
        <p class="empty">Choose a folder, or drop one here from the Finder.</p>
      {:else}
        {#each entries as entry (entry.path)}
          <!-- svelte-ignore a11y_click_events_have_key_events (the listbox handles keys) -->
          <div
            class="row"
            role="option"
            tabindex="-1"
            aria-selected={macSel.has(entry.path)}
            class:selected={macSel.has(entry.path)}
            class:dir={entry.kind === "dir"}
            onclick={(e) => clickMac(e, entry)}
            ondblclick={() =>
              entry.kind === "dir"
                ? setFolder(entry.path)
                : sendFiles([{ path: entry.path, name: entry.name, kind: entry.kind, size: entry.size }])}
            onpointerdown={(e) => entry.kind !== "dir" && pointerDown(e, "mac", entry.path)}
          >
            <span class="icon">{kindIcon[entry.kind]}</span>
            <span class="name">{entry.name}</span>
            <span class="size">{entry.kind === "dir" ? "" : formatBytes(entry.size)}</span>
          </div>
        {:else}
          <p class="empty">No .wav, .akp or .akm files here.</p>
        {/each}
      {/if}
    </div>
  </section>
</div>

<section class="queue">
  <div class="head">
    <span class="title">Queue</span>
    <span class="tools">
      <button onclick={() => queue.clearFinished()} disabled={queue.jobs.length === queue.pending}>Clear finished</button>
    </span>
  </div>
  <div class="jobs">
    {#each queue.jobs as job (job.id)}
      <div class="job {job.status}">
        <span class="dir">{job.direction === "get" ? "→ Mac" : "→ Sampler"}</span>
        <span class="name" title={job.label}>{job.label}{job.deps ? " +deps" : ""}</span>
        <span class="bar">
          <span
            style:width={job.status === "done"
              ? "100%"
              : job.total
                ? `${Math.min(100, (100 * job.done) / job.total)}%`
                : job.status === "running"
                  ? "100%"
                  : "0%"}
            class:indeterminate={job.status === "running" && !job.total}
          ></span>
        </span>
        <span class="state" title={job.note}>
          {#if job.status === "running"}
            {job.item && job.item !== job.source ? job.item : ""}
            {job.done ? formatBytes(job.done) : ""}
          {:else if job.status === "queued"}
            queued <button class="x" onclick={() => queue.cancel(job.id)} title="Remove from the queue">✕</button>
          {:else if job.status === "error"}
            {job.note}
          {:else}
            {job.status}{job.note ? ` · ${job.note}` : ""}
          {/if}
        </span>
      </div>
    {:else}
      <p class="empty">Drag items between the two sides, double-click them, or use Get and Send.</p>
    {/each}
  </div>
</section>

{#if drag}
  <div class="ghost" style:left="{drag.x + 12}px" style:top="{drag.y + 8}px">
    {drag.label}
    {#if drag.over && drag.over !== drag.from}<b>{drag.from === "sampler" ? "→ Mac" : "→ Sampler"}</b>{/if}
  </div>
{/if}

<style>
  .transfers {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    gap: 10px;
    padding: 10px 12px 0;
    height: calc(100% - 190px);
    min-height: 280px;
  }
  .dragging {
    cursor: grabbing;
  }
  .pane,
  .queue {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 6px;
  }
  .pane.target {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border-bottom: 1px solid var(--line);
  }
  .tools {
    margin-left: auto;
    display: flex;
    gap: 4px;
  }
  .tools button {
    padding: 1px 8px;
    font-size: 12px;
  }
  .path {
    margin: 6px 8px 0;
    overflow: hidden;
    text-overflow: ellipsis;
    direction: rtl;
    text-align: left;
  }
  .list {
    flex: 1;
    overflow: auto;
    padding: 4px 0;
    outline: none;
  }
  .group {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    padding: 8px 10px 2px;
    color: var(--muted);
  }
  .row {
    display: grid;
    grid-template-columns: 1fr auto 70px 28px;
    gap: 8px;
    align-items: center;
    padding: 2px 10px;
    min-height: 24px;
  }
  .pane[data-drop="mac"] .row {
    grid-template-columns: 16px 1fr 70px;
  }
  .row:nth-child(even) {
    background: var(--panel-2);
  }
  .row.selected {
    background: var(--select);
    color: var(--select-text);
  }
  .list:not(:focus-within) .row.selected {
    background: var(--select-soft);
    color: var(--text);
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--mono);
    font-size: 12px;
  }
  .detail,
  .size {
    font-size: 11px;
    opacity: 0.75;
    white-space: nowrap;
  }
  .size {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .icon {
    font-size: 11px;
    opacity: 0.7;
    text-align: center;
  }
  .dir .name {
    font-family: inherit;
  }
  .play {
    width: 24px;
    height: 20px;
    padding: 0;
    font-size: 9px;
  }
  .play.on {
    background: var(--accent);
    color: var(--accent-text);
    border-color: var(--accent);
  }
  .none,
  .empty {
    color: var(--muted);
    font-size: 12px;
    padding: 2px 10px;
  }
  .empty {
    padding: 16px;
  }
  .middle {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 8px;
    width: 150px;
  }
  .middle button {
    padding: 6px;
  }
  .middle label {
    font-size: 12px;
  }
  .queue {
    margin: 10px 12px 10px;
    height: 170px;
  }
  .jobs {
    overflow: auto;
    flex: 1;
  }
  .job {
    display: grid;
    grid-template-columns: 74px minmax(120px, 1.2fr) 1fr minmax(120px, 1.4fr);
    gap: 10px;
    align-items: center;
    padding: 2px 10px;
    font-size: 12px;
  }
  .job .dir {
    color: var(--muted);
  }
  .bar {
    height: 6px;
    background: var(--track);
    border-radius: 3px;
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width 0.1s;
  }
  .done .bar span {
    background: var(--ok);
  }
  .error .bar span {
    background: var(--bad);
    width: 100% !important;
  }
  .error .state {
    color: var(--bad);
  }
  .cancelled {
    opacity: 0.5;
  }
  .indeterminate {
    animation: pulse 1s ease-in-out infinite alternate;
  }
  @keyframes pulse {
    from {
      opacity: 0.35;
    }
    to {
      opacity: 1;
    }
  }
  .state {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted);
  }
  .x {
    border: none;
    background: none;
    padding: 0 4px;
    color: var(--muted);
  }
  .ghost {
    position: fixed;
    pointer-events: none;
    background: var(--select);
    color: var(--select-text);
    padding: 3px 8px;
    border-radius: 4px;
    font-size: 12px;
    box-shadow: 0 4px 14px rgb(0 0 0 / 0.25);
    z-index: 20;
  }
</style>
