<script lang="ts">
  // Asks which of a program's or multi's dependencies to copy along with it.
  import { formatBytes, type Line } from "./api";
  import { app } from "./state.svelte";
  import { describePlace, sourceLabel } from "./tree.svelte";
  import { estimate, transfers } from "./requests.svelte";

  const d = $derived(transfers.dialog);
  let checked = $state<Record<string, boolean>>({});
  let replace = $state(false);
  let dontAsk = $state(false);
  let form = $state<HTMLFormElement>();

  // New lines: tick what is missing at the destination (all, with replace).
  $effect(() => {
    const lines = d?.lines;
    if (!lines) return;
    replace = !!app.settings.replace;
    dontAsk = false;
    checked = Object.fromEntries(lines.map((l) => [l.file, l.found && (!l.exists || replace)]));
    queueMicrotask(() => form?.querySelector<HTMLElement>("button.primary")?.focus());
  });

  function setReplace(on: boolean) {
    replace = on;
    for (const l of d?.lines ?? []) if (l.found && l.exists) checked[l.file] = on;
  }

  const where = $derived(
    !d ? "" : d.dest.at === "memory" ? "in memory" : d.dest.at === "mac" ? "in the folder" : "on the disk",
  );
  const chosen = $derived((d?.lines ?? []).filter((l) => checked[l.file]));
  const bytes = $derived(chosen.reduce((n, l) => n + (l.size ?? 0), 0));
  const seconds = $derived(d ? estimate(d.item.source, d.dest, bytes) : null);
  const existing = $derived((d?.lines ?? []).filter((l) => l.found && l.exists).length);
  const what = $derived(d?.item.kind === "multi" ? "programs and samples" : "samples");

  function depth(l: Line) {
    if (l.main) return 0;
    return l.parent ? 2 : 1;
  }

  function status(l: Line) {
    if (!l.found) return "not found at the source";
    if (l.exists) return checked[l.file] ? `${where} · will replace` : where;
    return "";
  }

  function time(s: number) {
    return s < 60 ? `about ${Math.max(1, Math.round(s))} s` : `about ${Math.round(s / 60)} min`;
  }

  function submit(e: SubmitEvent) {
    e.preventDefault();
    transfers.answer({ files: chosen.map((l) => l.file), replace, dontAsk });
  }
</script>

{#if d}
  <div class="scrim" role="presentation" onkeydown={(e) => e.key === "Escape" && transfers.answer("cancel")}>
    <form class="dialog" bind:this={form} onsubmit={submit} aria-label="Dependencies">
      <h2 class="sign">
        <span class="t">Copy {sourceLabel(d.item.source)} to {describePlace(d.dest)}</span>
        {#if d.count > 1}<span class="count">{d.index} of {d.count}</span>{/if}
      </h2>
      <div class="body">
        {#if d.error}
          <p class="error">{d.error}</p>
        {:else if !d.lines}
          <p class="muted">Reading the {what} {sourceLabel(d.item.source)} uses…</p>
        {:else}
          <p>
            {d.lines.length > 1
              ? `It uses these ${what}. Ticked items are copied; those already ${where} are left unticked.`
              : `It uses no ${what}.`}
          </p>
          <div class="lines" role="list">
            {#each d.lines as l (l.file)}
              <label
                class="line"
                class:main={l.main}
                class:off={!l.found}
                role="listitem"
                style:padding-left="{8 + depth(l) * 16}px"
              >
                <input type="checkbox" bind:checked={checked[l.file]} disabled={!l.found} />
                <span class="name">{l.file}</span>
                <span class="status" class:exists={l.exists && l.found}>{status(l)}</span>
                <span class="size">{l.size === null ? "" : formatBytes(l.size)}</span>
              </label>
            {/each}
          </div>
          <div class="options">
            <label title="Tick the items that already exist, so they are replaced">
              <input type="checkbox" checked={replace} disabled={!existing} onchange={(e) => setReplace(e.currentTarget.checked)} />
              Replace existing{existing ? ` (${existing})` : ""}
            </label>
            <label title="For the rest of this session, copy what is missing (or everything, with Replace) without asking">
              <input type="checkbox" bind:checked={dontAsk} />
              Don't ask again this session
            </label>
          </div>
        {/if}
        <div class="buttons">
          <span class="summary muted">
            {#if d.lines}
              {chosen.length} of {d.lines.filter((l) => l.found).length} files{bytes ? ` · ${formatBytes(bytes)}` : ""}{seconds !== null && bytes
                ? ` · ${time(seconds)}`
                : ""}
            {/if}
          </span>
          <button type="button" onclick={() => transfers.answer("cancel")}>{d.count > 1 ? "Cancel all" : "Cancel"}</button>
          {#if d.count > 1}<button type="button" onclick={() => transfers.answer("skip")}>Skip</button>{/if}
          <button type="submit" class="primary" disabled={!d.lines || !chosen.length}>
            Copy {chosen.length || ""}
          </button>
        </div>
      </div>
    </form>
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.4);
    display: grid;
    place-items: center;
    z-index: 10;
  }
  .dialog {
    background: var(--surface);
    border: 1px solid var(--rule);
    width: min(560px, calc(100vw - 32px));
  }
  h2 {
    margin: 0;
    font-size: 14px;
    padding: 6px 12px;
  }
  .t {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .count {
    margin-left: auto;
    color: var(--sign-muted);
    font-size: 12px;
    white-space: nowrap;
  }
  .body {
    padding: 12px;
  }
  p {
    margin: 0 0 8px;
  }
  .error {
    color: var(--bad);
    font-weight: 700;
    -webkit-user-select: text;
    user-select: text;
  }
  .lines {
    max-height: 300px;
    overflow: auto;
    border-top: 1px solid var(--rule);
    border-bottom: 1px solid var(--rule-soft);
  }
  .line {
    display: grid;
    grid-template-columns: auto 1fr auto 64px;
    gap: 8px;
    align-items: center;
    min-height: 24px;
    padding-right: 8px;
    border-bottom: 1px solid var(--rule-soft);
  }
  .line:last-child {
    border-bottom: none;
  }
  .line.main .name {
    font-weight: 700;
  }
  .line.off {
    color: var(--muted);
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .status {
    font-size: 11px;
    color: var(--muted);
    white-space: nowrap;
  }
  .off .status {
    color: var(--bad);
  }
  .size {
    text-align: right;
    font-size: 11px;
    color: var(--muted);
  }
  .options {
    display: flex;
    gap: 16px;
    margin-top: 8px;
    font-size: 12px;
  }
  .buttons {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 12px;
    padding-top: 12px;
    border-top: 1px solid var(--rule-soft);
  }
  .summary {
    margin-right: auto;
    font-size: 12px;
  }
</style>
