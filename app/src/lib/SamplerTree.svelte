<script lang="ts">
  // The samplers' memory and disks as a tree. Selection, dragging and keys
  // are handled by Transfers.svelte through the callbacks.
  import { formatBytes, type Place } from "./api";
  import { app } from "./state.svelte";
  import { transfers } from "./requests.svelte";
  import { tree, type Row } from "./tree.svelte";

  interface Props {
    rows: Row[];
    selected: Set<string>;
    focus: string | null;
    /** The current drop target, highlighted. */
    target: Place | null;
    playing: { sampler: number; name: string } | null;
    onrowdown: (e: PointerEvent, row: Row) => void;
    onrowclick: (e: MouseEvent, row: Row) => void;
    onrowdbl: (row: Row) => void;
    onplay: (row: Row) => void;
    onkeydown: (e: KeyboardEvent) => void;
  }
  let { rows, selected, focus, target, playing, onrowdown, onrowclick, onrowdbl, onplay, onkeydown }: Props = $props();

  const targetJson = $derived(target ? JSON.stringify(target) : null);
  const bullets = ["purple", "lime", "brown", "grey", "orange", "blue"];
  function bullet(id: number) {
    const i = app.samplers.findIndex((s) => s.id === id);
    return { n: i + 1, color: bullets[i % bullets.length] };
  }
  const icon: Record<string, string> = { sample: "∿", program: "P", multi: "M", other: "·" };

  function json(p: Place | null) {
    return p ? JSON.stringify(p) : undefined;
  }
</script>

<div class="tree" role="tree" aria-multiselectable="true" tabindex="0" {onkeydown}>
  {#if !rows.length}
    <p class="empty">No sampler connected. Press + to add a USB sampler or a simulator.</p>
  {/if}
  {#each rows as row (row.key)}
    {@const drop = json(row.drop)}
    {#if row.type === "root"}
      {@const b = bullet(row.sampler)}
      {@const s = app.sampler(row.sampler)}
      <!-- svelte-ignore a11y_click_events_have_key_events (the tree handles keys) -->
      <div
        class="root sign"
        class:target={drop !== undefined && drop === targetJson}
        class:lost={s?.status !== "connected"}
        role="treeitem"
        tabindex="-1"
        aria-selected={focus === row.key}
        aria-expanded={row.expanded}
        data-key={row.key}
        data-drop={drop}
        onclick={(e) => onrowclick(e, row)}
      >
        <button class="disc" tabindex="-1" onclick={(e) => (e.stopPropagation(), tree.toggle(row))} aria-label="Expand">
          {row.expanded ? "▾" : "▸"}
        </button>
        <span class="bullet small {b.color}">{b.n}</span>
        <span class="label">{row.label}</span>
        <span class="status">{s?.loading ? "reading memory…" : row.detail}</span>
        <span class="tools">
          {#if s?.status !== "connected"}
            <button onclick={(e) => (e.stopPropagation(), app.reconnect(row.sampler))}>Reconnect</button>
          {:else if s?.active}
            <span class="active" title="The front panel, meters and program editor use this sampler">Active</span>
          {:else}
            <button
              onclick={(e) => (e.stopPropagation(), app.setActive(row.sampler))}
              title="Use this sampler for the front panel, meters and program editor">Make active</button
            >
          {/if}
          <button
            class="x"
            title="Disconnect {row.label}"
            aria-label="Disconnect"
            onclick={(e) => (e.stopPropagation(), tree.forget(row.sampler), app.remove(row.sampler))}>✕</button
          >
        </span>
      </div>
    {:else}
      <!-- svelte-ignore a11y_click_events_have_key_events (the tree handles keys) -->
      <div
        class="row {row.type}"
        class:selected={selected.has(row.key)}
        class:focus={focus === row.key}
        class:target={drop !== undefined && drop === targetJson}
        class:draggable={row.source !== null}
        role="treeitem"
        tabindex="-1"
        aria-selected={selected.has(row.key)}
        aria-expanded={row.expandable ? row.expanded : undefined}
        data-key={row.key}
        data-drop={drop}
        style:--depth={row.depth}
        onclick={(e) => onrowclick(e, row)}
        ondblclick={() => onrowdbl(row)}
        onpointerdown={(e) => onrowdown(e, row)}
      >
        <span class="disc" role="presentation" onclick={(e) => (e.stopPropagation(), tree.toggle(row))}>
          {row.expandable ? (row.expanded ? "▾" : "▸") : ""}
        </span>
        {#if row.type === "group"}
          <span class="label">{row.label}</span>
          <span class="count">{row.detail}</span>
          {#if row.group === "disks"}
            <span class="experimental" title="Disk loads, saves and deletes are untested on hardware">Experimental</span>
          {/if}
          {#if row.group === "disks" && row.expanded}
            <button
              class="mini"
              title="Read the disk list again"
              onclick={(e) => (e.stopPropagation(), tree.loadDisks(row.sampler))}
              onpointerdown={(e) => e.stopPropagation()}>Rescan</button
            >
          {/if}
        {:else if row.type === "note"}
          <span class="label note" class:error={row.note === "error"}>{row.label}</span>
        {:else}
          {#if row.type === "file" || row.type === "item"}
            <span class="icon">{icon[row.kind ?? "other"]}</span>
          {:else}
            <span class="icon">{row.type === "disk" ? "⛁" : "▭"}</span>
          {/if}
          <span class="label" class:other={row.kind === "other"}>{row.label}</span>
          <span class="detail">{row.detail}</span>
          <span class="size">{row.size === null ? "" : formatBytes(row.size)}</span>
          <span class="act">
            {#if row.source && transfers.isStaged(row.source)}
              <span class="ready" title="Fetched: drags to the Finder start at once">⇢</span>
            {/if}
            {#if row.type === "item" && row.kind === "sample"}
              {@const on = playing?.sampler === row.sampler && playing.name === row.name}
              <button
                class="play"
                class:on
                title={on ? "Stop" : "Play on the sampler"}
                onclick={(e) => (e.stopPropagation(), onplay(row))}
                onpointerdown={(e) => e.stopPropagation()}>{on ? "■" : "▶"}</button
              >
            {/if}
          </span>
        {/if}
      </div>
    {/if}
  {/each}
</div>

<style>
  .tree {
    flex: 1;
    overflow: auto;
    outline: none;
    padding-bottom: 8px;
  }
  .root {
    margin-top: 8px;
    gap: 6px;
    padding: 3px 4px 3px 2px;
    position: sticky;
    top: 0;
    z-index: 1;
  }
  .root:first-child {
    margin-top: 0;
  }
  .root .label {
    font-size: 13px;
  }
  .root .status {
    color: var(--sign-muted);
    font-weight: 400;
    font-size: 11px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .root .tools {
    margin-left: auto;
    display: flex;
    gap: 4px;
    align-items: center;
  }
  .root button {
    background: transparent;
    color: var(--sign-text);
    border-color: var(--sign-muted);
    padding: 0 6px;
    font-size: 11px;
  }
  .root button:hover:not(:disabled) {
    background: transparent;
    border-color: var(--sign-text);
  }
  .root .disc,
  .root .x {
    border: none;
    padding: 0 4px;
  }
  .root .active {
    font-size: 11px;
    color: var(--sign-muted);
  }
  .root.lost .label {
    text-decoration: line-through;
  }
  .root.target {
    background: var(--accent);
  }
  .row {
    display: grid;
    grid-template-columns: 14px 16px minmax(60px, 1fr) auto 64px 40px;
    gap: 6px;
    align-items: center;
    min-height: 23px;
    padding: 0 8px 0 calc(4px + var(--depth) * 14px);
    border-bottom: 1px solid var(--rule-soft);
  }
  .row.group {
    grid-template-columns: 14px auto 1fr auto;
    font-weight: 700;
    font-size: 12px;
  }
  .row.note {
    grid-template-columns: 14px 1fr;
  }
  .row.selected {
    background: var(--select);
    color: var(--select-text);
  }
  .tree:not(:focus-within) .row.selected {
    background: var(--select-soft);
    color: var(--text);
  }
  .row.focus:not(.selected) {
    background: var(--surface-2);
  }
  .row.target:not(.selected) {
    background: var(--select-soft);
  }
  .disc {
    font-size: 10px;
    text-align: center;
    color: var(--muted);
  }
  .selected .disc,
  .selected .detail,
  .selected .size,
  .selected .icon {
    color: inherit;
  }
  .count {
    color: var(--muted);
    font-weight: 400;
  }
  /* A small yellow caution sign, as on the subway: black on yellow, square. */
  .experimental {
    background: var(--warn);
    color: #000;
    font-size: 10px;
    font-weight: 700;
    padding: 1px 5px;
    margin-left: 6px;
    flex: none;
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .label.other,
  .note {
    color: var(--muted);
  }
  .note.error {
    color: var(--bad);
  }
  .icon {
    font-size: 11px;
    text-align: center;
    color: var(--muted);
  }
  .detail,
  .size {
    font-size: 11px;
    color: var(--muted);
    white-space: nowrap;
  }
  .size {
    text-align: right;
  }
  .act {
    display: flex;
    justify-content: flex-end;
    gap: 4px;
    align-items: center;
  }
  .ready {
    font-size: 12px;
    color: var(--accent);
  }
  .selected .ready {
    color: inherit;
  }
  .play {
    width: 22px;
    height: 18px;
    padding: 0;
    font-size: 8px;
  }
  .play.on {
    background: var(--accent);
    color: var(--accent-text);
    border-color: var(--accent);
  }
  .mini {
    padding: 0 6px;
    font-size: 11px;
    font-weight: 700;
  }
  .empty {
    color: var(--muted);
    padding: 16px;
    margin: 0;
  }
</style>
