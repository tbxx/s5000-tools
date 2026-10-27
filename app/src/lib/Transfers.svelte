<script lang="ts">
  // Transfers: the samplers' memory and disks as a tree on the left, a Mac
  // folder on the right, the queue below. Drag between any of them, to and
  // from the Finder, and between samplers; files go where their type
  // belongs (.wav to Samples, .akp to Programs, .akm to Multis).
  import { onMount, untrack } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { zoom } from "./prefs.svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { startDrag } from "@crabnebula/tauri-plugin-drag";
  import { api, formatBytes, message, usbLabel, type Entry, type Kind, type Place, type UsbDevice } from "./api";
  import DepDialog from "./DepDialog.svelte";
  import SamplerTree from "./SamplerTree.svelte";
  import { dialogs } from "./dialogs.svelte";
  import { acceptDiskOps } from "./experimental";
  import { queue, type Job } from "./queue.svelte";
  import { app } from "./state.svelte";
  import { describePlace, folderKey, tree, type Row } from "./tree.svelte";
  import { canDrop, transfers, type Transferable } from "./requests.svelte";

  type Pane = "tree" | "mac";

  // --- Tree ------------------------------------------------------------------------
  const rows = $derived(tree.rows(app.samplers));
  const sel = new SvelteSet<string>();
  let anchor = $state<string | null>(null);
  let focus = $state<string | null>(null);
  let playing = $state<{ sampler: number; name: string } | null>(null);
  const selectedRows = $derived(rows.filter((r) => sel.has(r.key)));
  const selectedItems = $derived(selectedRows.filter((r) => r.source));
  /** The sampler the tree's buttons act on: the focused row's, or the active one. */
  const ctxSampler = $derived(rows.find((r) => r.key === focus)?.sampler ?? app.active?.id ?? null);
  const ctxConnected = $derived(ctxSampler !== null && app.sampler(ctxSampler)?.status === "connected");

  // Drop selection of rows that went away.
  $effect(() => {
    const keys = new Set(rows.map((r) => r.key));
    untrack(() => {
      for (const k of [...sel]) if (!keys.has(k)) sel.delete(k);
    });
  });

  function asTransferable(r: Row): Transferable {
    return { source: r.source!, kind: r.kind as Kind, size: r.size };
  }

  // --- Mac side --------------------------------------------------------------------
  let entries = $state<Entry[]>([]);
  const macSel = new SvelteSet<string>();
  let macAnchor = $state<string | null>(null);
  const folder = $derived(app.settings.folder ?? "");
  const selectedFiles = $derived(entries.filter((e) => macSel.has(e.path) && e.kind !== "dir"));
  const macPlace = $derived<Place | null>(folder ? { at: "mac", folder } : null);

  async function loadFolder() {
    if (!folder) return;
    try {
      entries = await api.listFolder(folder);
      for (const p of [...macSel]) if (!entries.some((e) => e.path === p)) macSel.delete(p);
    } catch (e) {
      entries = [];
      app.fail(e);
    }
  }

  // Reload when the folder changes or a transfer finishes.
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
    macSel.clear();
    app.save({ folder: path });
  }

  function up() {
    setFolder(folder.replace(/\/[^/]+\/?$/, "") || "/");
  }

  function macTransferables(files = selectedFiles): Transferable[] {
    return transfers.fromPaths(
      files.map((f) => f.path),
      new Map(files.map((f) => [f.path, f.size])),
    );
  }

  // --- Selection ---------------------------------------------------------------------
  function pick(e: { shiftKey: boolean; metaKey: boolean; ctrlKey: boolean }, key: string, order: string[], set: SvelteSet<string>, from: string | null): string | null {
    if (e.shiftKey && from) {
      const [a, b] = [order.indexOf(from), order.indexOf(key)].sort((x, y) => x - y);
      set.clear();
      for (const k of order.slice(a, b + 1)) set.add(k);
      return from;
    }
    if (e.metaKey || e.ctrlKey) {
      if (set.has(key)) set.delete(key);
      else set.add(key);
      return key;
    }
    set.clear();
    set.add(key);
    return key;
  }

  const selectable = (r: Row) => r.type === "item" || r.type === "file" || r.type === "folder" || r.type === "disk";

  function clickRow(e: MouseEvent, row: Row) {
    focus = row.key;
    if (!selectable(row)) {
      if (row.type === "group" || row.type === "root") {
        if (!(e.target as HTMLElement).closest("button")) tree.toggle(row);
      }
      return;
    }
    anchor = pick(e, row.key, rows.filter(selectable).map((r) => r.key), sel, anchor);
  }

  function dblRow(row: Row) {
    if (row.type === "item" && row.source) send([asTransferable(row)], macPlace);
    else if (row.type === "file" && row.source) send([asTransferable(row)], { at: "memory", sampler: row.sampler });
    else if (row.expandable) tree.toggle(row);
  }

  function clickMac(e: MouseEvent, entry: Entry) {
    macAnchor = pick(e, entry.path, entries.map((x) => x.path), macSel, macAnchor);
  }

  function dblMac(entry: Entry) {
    if (entry.kind === "dir") return setFolder(entry.path);
    const to = app.active;
    if (to) send(macTransferables([entry]), { at: "memory", sampler: to.id });
    else app.fail("Connect a sampler first.");
  }

  // --- Actions -------------------------------------------------------------------------
  function send(items: Transferable[], dest: Place | null) {
    if (!dest) return app.fail("Choose a folder on the Mac first.");
    if (!items.length) return;
    if (!canDrop(items, dest)) return app.fail(`Already in ${describePlace(dest)}.`);
    void transfers.send(items, dest);
  }

  const getSelected = () => send(selectedItems.map(asTransferable), macPlace);
  const sendTo = $derived(app.active && app.active.status === "connected" ? app.active : null);
  function sendSelected() {
    if (!sendTo) return app.fail("Connect a sampler first.");
    send(macTransferables(), { at: "memory", sampler: sendTo.id });
  }

  async function play(row: Row) {
    const name = row.name!;
    try {
      if (playing) await api.audition(playing.sampler, playing.name, false);
      if (playing?.sampler === row.sampler && playing.name === name) {
        playing = null;
      } else {
        await api.audition(row.sampler, name, true);
        playing = { sampler: row.sampler, name };
      }
    } catch (e) {
      playing = null;
      app.fail(e);
    }
  }

  function list(names: string[]) {
    return names.length > 8 ? `${names.slice(0, 8).join(", ")} and ${names.length - 8} more` : names.join(", ");
  }

  async function remove() {
    const items = selectedRows.filter((r) => r.type === "item");
    const files = selectedRows.filter((r) => r.type === "file" || r.type === "folder");
    if (!items.length && !files.length) return;
    const names = [...items, ...files].map((r) => r.label);
    const where = items.length && files.length ? "memory and disk" : items.length ? "memory" : "the disk";
    const ok = await dialogs.confirm(
      `Delete ${names.length} item${names.length === 1 ? "" : "s"}?`,
      `${list(names)}\n\nThis removes them from the sampler's ${where}. It cannot be undone.${files.some((f) => f.type === "folder") ? "\nOnly empty folders can be deleted." : ""}`,
      "Delete",
      true,
    );
    if (!ok || (files.length && !(await acceptDiskOps()))) return;
    try {
      const samplers = new Set(items.map((r) => r.sampler));
      for (const s of samplers) {
        for (const kind of ["multi", "program", "sample"] as Kind[]) {
          const n = items.filter((r) => r.sampler === s && r.kind === kind).map((r) => r.name!);
          if (n.length) await api.deleteItems(s, kind, n);
        }
      }
      // Disk files and folders, grouped by the folder they are in.
      const groups = new Map<string, Row[]>();
      for (const f of files) {
        const at = f.type === "folder" ? f.path!.slice(0, -1) : f.path!;
        const k = folderKey(f.sampler, f.disk!, at);
        groups.set(k, [...(groups.get(k) ?? []), f]);
      }
      for (const g of groups.values()) {
        const f = g[0];
        const at = f.type === "folder" ? f.path!.slice(0, -1) : f.path!;
        await api.diskDelete(
          f.sampler,
          f.disk!,
          at,
          g.filter((x) => x.type === "file").map((x) => x.name!),
          g.filter((x) => x.type === "folder").map((x) => x.name!),
        );
        tree.reload({ at: "disk", sampler: f.sampler, disk: f.disk!, path: at });
        transfers.invalidate(f.sampler);
      }
      app.say(`Deleted ${list(names)}`);
      sel.clear();
      for (const s of samplers) {
        transfers.invalidate(s);
        await app.refresh(s);
      }
    } catch (e) {
      app.fail(e);
    }
  }

  async function rename() {
    const [row] = selectedRows;
    if (!row || selectedRows.length !== 1 || row.type !== "item") return;
    const v = await dialogs.ask(`Rename ${row.kind}`, [{ label: "Name", value: row.name! }], "Rename");
    if (!v || !v[0].trim() || v[0] === row.name) return;
    try {
      await api.rename(row.sampler, row.kind as Kind, row.name!, v[0]);
      app.say(`Renamed ${row.name} to ${v[0]}`);
      transfers.invalidate(row.sampler);
    } catch (e) {
      app.fail(e);
    }
    await app.refresh(row.sampler);
  }

  async function newProgram() {
    if (ctxSampler === null) return;
    const v = await dialogs.ask(`New program in ${app.label(ctxSampler)}`, [
      { label: "Name", value: "New Program" },
      { label: "Keygroups", value: "1" },
    ], "Create");
    if (!v) return;
    const kgs = Math.max(1, Math.min(99, parseInt(v[1]) || 1));
    await api.createProgram(ctxSampler, v[0], kgs).catch((e) => app.fail(e));
    await app.refresh(ctxSampler);
  }

  async function newMulti() {
    if (ctxSampler === null) return;
    const v = await dialogs.ask(`New multi in ${app.label(ctxSampler)}`, [
      { label: "Name", value: "New Multi" },
      { label: "Parts", value: "32", options: ["32", "64", "128"] },
    ], "Create");
    if (!v) return;
    await api.createMulti(ctxSampler, v[0], parseInt(v[1])).catch((e) => app.fail(e));
    await app.refresh(ctxSampler);
  }

  /** The disk folder the focused row is in or is. */
  const ctxFolder = $derived.by(() => {
    const r = rows.find((x) => x.key === focus);
    return r?.drop?.at === "disk" ? r.drop : null;
  });

  async function newFolder() {
    const f = ctxFolder;
    if (!f || f.at !== "disk" || !(await acceptDiskOps())) return;
    const v = await dialogs.ask(`New folder in ${describePlace(f)}`, [{ label: "Name", value: "NEW FOLDER" }], "Create");
    if (!v || !v[0].trim()) return;
    try {
      await api.diskCreateFolder(f.sampler, f.disk, f.path, v[0].trim());
      tree.reload(f);
    } catch (e) {
      app.fail(e);
    }
  }

  async function refresh() {
    for (const s of app.samplers) {
      if (s.status !== "connected") continue;
      await app.refresh(s.id);
      if (tree.disks.has(s.id)) await tree.loadDisks(s.id);
    }
    for (const k of [...tree.folders.keys()]) {
      const [sampler, , disk, path] = k.split("|");
      void tree.loadFolder(Number(sampler), Number(disk), path ? path.split("/") : []);
    }
  }

  // --- Adding samplers ---------------------------------------------------------------------
  let addOpen = $state(false);
  let usb = $state<UsbDevice[] | null>(null);
  let usbError = $state<string | null>(null);

  async function openAdd() {
    addOpen = !addOpen;
    if (!addOpen) return;
    usb = null;
    usbError = null;
    try {
      usb = await api.usbDevices();
    } catch (e) {
      usbError = message(e);
    }
  }

  async function add(origin: { kind: "simulator" } | { kind: "usb"; key: string }) {
    addOpen = false;
    const s = await app.add(origin);
    if (s) focus = `${s.id}|root`;
  }

  // --- Keys --------------------------------------------------------------------------------
  function scrollTo(key: string) {
    queueMicrotask(() => document.querySelector(`[data-key="${CSS.escape(key)}"]`)?.scrollIntoView({ block: "nearest" }));
  }

  function treeKey(e: KeyboardEvent) {
    const i = rows.findIndex((r) => r.key === focus);
    const row = rows[i];
    const move = (j: number) => {
      const r = rows[Math.max(0, Math.min(rows.length - 1, j))];
      if (!r) return;
      focus = r.key;
      if (selectable(r)) {
        if (e.shiftKey && anchor) pick(e, r.key, rows.filter(selectable).map((x) => x.key), sel, anchor);
        else {
          sel.clear();
          sel.add(r.key);
          anchor = r.key;
        }
      }
      scrollTo(r.key);
    };
    if (e.key === "ArrowDown") (e.preventDefault(), move(i + 1));
    else if (e.key === "ArrowUp") (e.preventDefault(), move(i < 0 ? 0 : i - 1));
    else if (e.key === "ArrowRight" && row) {
      e.preventDefault();
      if (row.expandable && !row.expanded) tree.setOpen(row, true);
      else move(i + 1);
    } else if (e.key === "ArrowLeft" && row) {
      e.preventDefault();
      if (row.expandable && row.expanded) tree.setOpen(row, false);
      else {
        let j = i - 1;
        while (j >= 0 && rows[j].depth >= row.depth) j--;
        if (j >= 0) move(j);
      }
    } else if (e.key === "Backspace" || e.key === "Delete") remove();
    else if (e.key === "Enter") getSelected();
    else if ((e.metaKey || e.ctrlKey) && e.key === "a") {
      e.preventDefault();
      sel.clear();
      for (const r of rows) if (r.source) sel.add(r.key);
    } else if (e.key === " " && selectedRows.length === 1 && selectedRows[0].kind === "sample" && selectedRows[0].type === "item") {
      e.preventDefault();
      play(selectedRows[0]);
    }
  }

  function macKey(e: KeyboardEvent) {
    if (e.key === "Enter") sendSelected();
    else if ((e.metaKey || e.ctrlKey) && e.key === "a") {
      e.preventDefault();
      macSel.clear();
      for (const x of entries) if (x.kind !== "dir") macSel.add(x.path);
    }
  }

  // --- Dragging (pointer based: between panes, samplers and folders) --------------------
  interface Drag {
    id: number;
    from: Pane;
    items: Transferable[];
    label: string;
    x: number;
    y: number;
    target: Place | null;
    /** Over the Finder zone or outside the window. */
    out: boolean;
    finder: "idle" | "fetching" | "error";
  }
  let drag = $state<Drag | null>(null);
  let pressed: { from: Pane; x: number; y: number } | null = null;
  let dragIds = 0;

  function placeAt(x: number, y: number): Place | null {
    const el = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-drop]");
    return el?.dataset.drop ? (JSON.parse(el.dataset.drop) as Place) : null;
  }

  function overFinderZone(x: number, y: number) {
    return !!document.elementFromPoint(x, y)?.closest("[data-finder]");
  }

  function rowDown(e: PointerEvent, row: Row) {
    if (e.button !== 0 || e.shiftKey || e.metaKey || e.ctrlKey || !row.source) return;
    if (!sel.has(row.key)) {
      sel.clear();
      sel.add(row.key);
      anchor = row.key;
    }
    focus = row.key;
    pressed = { from: "tree", x: e.clientX, y: e.clientY };
  }

  function macDown(e: PointerEvent, entry: Entry) {
    if (e.button !== 0 || e.shiftKey || e.metaKey || e.ctrlKey || entry.kind === "dir") return;
    if (!macSel.has(entry.path)) {
      macSel.clear();
      macSel.add(entry.path);
      macAnchor = entry.path;
    }
    pressed = { from: "mac", x: e.clientX, y: e.clientY };
  }

  function pointerMove(e: PointerEvent) {
    if (!pressed) return;
    if (!drag) {
      if (Math.hypot(e.clientX - pressed.x, e.clientY - pressed.y) < 5) return;
      const items = pressed.from === "tree" ? selectedItems.map(asTransferable) : macTransferables();
      if (!items.length) return;
      const first = pressed.from === "tree" ? selectedItems[0].label : selectedFiles[0].name;
      drag = { id: ++dragIds, from: pressed.from, items, label: items.length === 1 ? first : `${items.length} items`, x: 0, y: 0, target: null, out: false, finder: "idle" };
    }
    drag.x = e.clientX;
    drag.y = e.clientY;
    const outside = e.clientX <= 0 || e.clientY <= 0 || e.clientX >= innerWidth - 1 || e.clientY >= innerHeight - 1;
    drag.out = outside || overFinderZone(e.clientX, e.clientY);
    drag.target = drag.out ? null : placeAt(e.clientX, e.clientY);
    if (drag.out && drag.finder === "idle") void toFinder(drag);
  }

  function pointerUp() {
    if (drag && !drag.out && drag.target && canDrop(drag.items, drag.target)) void transfers.send(drag.items, drag.target);
    else if (drag?.finder === "fetching") app.say(`Fetching ${drag.label} for the Finder; when it is ready, drag it out again.`);
    drag = null;
    pressed = null;
  }

  /** A small sign as the drag image. */
  function dragIcon(label: string, n: number): string {
    const c = document.createElement("canvas");
    const scale = 2;
    const ctx = c.getContext("2d");
    if (!ctx) return "";
    ctx.font = `700 ${12 * scale}px "Helvetica Neue", Helvetica, Arial, sans-serif`;
    const text = n > 1 ? `${n} files` : label;
    c.width = Math.min(260 * scale, ctx.measureText(text).width + 20 * scale);
    c.height = 22 * scale;
    ctx.fillStyle = "#000";
    ctx.fillRect(0, 0, c.width, c.height);
    ctx.fillStyle = "#fff";
    ctx.font = `700 ${12 * scale}px "Helvetica Neue", Helvetica, Arial, sans-serif`;
    ctx.textBaseline = "middle";
    ctx.fillText(text, 10 * scale, c.height / 2, c.width - 20 * scale);
    return c.toDataURL("image/png");
  }

  // macOS drags sampler items out as file promises: nothing is fetched until
  // they are dropped, and then straight into the folder they were dropped in.
  const promiseDrags = /Macintosh/.test(navigator.userAgent);
  /** Promise drags in progress, by token. */
  const dragsOut = new Map<number, Drag>();

  function pngBytes(dataUrl: string): number[] {
    const bin = atob(dataUrl.slice(dataUrl.indexOf(",") + 1));
    return Array.from(bin, (c) => c.charCodeAt(0));
  }

  async function promiseDrag(d: Drag) {
    const icon = pngBytes(dragIcon(d.label, d.items.length));
    drag = null;
    pressed = null;
    try {
      dragsOut.set(await api.dragOutStart(d.items.map((i) => i.source), icon), d);
    } catch (e) {
      app.fail(e);
    }
  }

  async function droppedInFinder(token: number, folder: string) {
    const d = dragsOut.get(token);
    dragsOut.delete(token);
    if (!d) return void api.dragOutFinish(token, "cancelled");
    try {
      await transfers.toFinderFolder(d.items, folder);
      await api.dragOutFinish(token, null);
      app.say(`Copied ${d.label} to ${folder}`);
    } catch (e) {
      await api.dragOutFinish(token, message(e) === "cancelled" ? "cancelled" : message(e)).catch(() => {});
      if (message(e) !== "cancelled") app.fail(e);
    }
  }

  /** Leaves the window: hand the files to a native drag, fetching them first if needed. */
  async function toFinder(d: Drag) {
    if (promiseDrags && d.items.every((i) => i.source.at !== "mac")) return promiseDrag(d);
    let paths = transfers.ready(d.items);
    if (!paths) {
      d.finder = "fetching";
      try {
        paths = await transfers.stage(d.items);
      } catch (e) {
        if (drag?.id === d.id) drag.finder = "error";
        if (message(e) !== "cancelled") app.fail(e);
        return;
      }
      // Released meanwhile: the files are ready for the next drag.
      if (drag?.id !== d.id) {
        app.say(`${d.label} is ready: drag it to the Finder again.`);
        return;
      }
    }
    const n = paths.length;
    const icon = dragIcon(d.label, n);
    drag = null;
    pressed = null;
    try {
      await startDrag({ item: paths, icon }, ({ result }) => {
        if (result === "Dropped") app.say(`Copied ${n} file${n === 1 ? "" : "s"} to the Finder`);
      });
    } catch (e) {
      app.fail(e);
    }
  }

  /** Fetches for the Finder: the jobs of the current drag. */
  const finderJobs = $derived(queue.jobs.filter((j) => j.forFinder && (j.status === "running" || j.status === "queued")));
  const finderWaiting = $derived(finderJobs.length > 0 && finderJobs.every((j) => j.status === "queued"));
  const finderPct = $derived.by(() => {
    const total = finderJobs.reduce((n, j) => n + (j.total ?? 0), 0);
    const done = finderJobs.reduce((n, j) => n + Math.min(j.done, j.total ?? j.done), 0);
    return total ? Math.round((100 * done) / total) : null;
  });

  // --- Files dropped from the Finder ----------------------------------------------------------
  let finderOver = $state<Place | null>(null);

  let screenScale = window.devicePixelRatio || 1;
  onMount(() => {
    getCurrentWindow()
      .scaleFactor()
      .then((f) => (screenScale = f))
      .catch(() => {});
    getCurrentWindow()
      .onScaleChanged(({ payload }) => (screenScale = payload.scaleFactor))
      .catch(() => {});
    const unlisten = getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (payload.type === "leave") {
        finderOver = null;
        return;
      }
      // Physical pixels to page pixels: the screen's scale, then the app's
      // zoom (devicePixelRatio includes the zoom on some engines, not others).
      const r = screenScale * zoom();
      const place = placeAt(payload.position.x / r, payload.position.y / r);
      if (payload.type !== "drop") {
        finderOver = place;
        return;
      }
      finderOver = null;
      if (!place) return;
      if (place.at === "mac") {
        if (payload.paths.length === 1)
          api
            .listFolder(payload.paths[0])
            .then(() => setFolder(payload.paths[0]))
            .catch(() => app.fail("Drop a folder here to open it, or drop files on a sampler."));
        return;
      }
      const items = transfers.fromPaths(payload.paths);
      if (items.length) send(items, place);
    });
    const unlistenDrop = listen<{ token: number; folder: string }>("drag-out-drop", ({ payload }) =>
      droppedInFinder(payload.token, payload.folder),
    );
    const unlistenEnd = listen<{ token: number; dropped: boolean }>("drag-out-end", ({ payload }) => {
      if (!payload.dropped) dragsOut.delete(payload.token);
    });
    return () => {
      unlisten.then((f) => f());
      unlistenDrop.then((f) => f());
      unlistenEnd.then((f) => f());
      if (playing) api.audition(playing.sampler, playing.name, false).catch(() => {});
      if (transfers.dialog) transfers.answer("cancel");
    };
  });

  const target = $derived(drag ? (canDrop(drag.items, drag.target) ? drag.target : null) : finderOver);
  const busy = $derived(queue.running);

  function jobState(job: Job) {
    if (job.status === "queued") return "queued";
    if (job.status === "running") {
      const step = job.steps > 1 ? `${job.step}/${job.steps} ` : "";
      const item = job.item && job.item !== job.label ? `${job.item} ` : "";
      return `${step}${item}${job.done ? formatBytes(job.done) : ""}`;
    }
    if (job.status === "error") return job.note;
    return job.status + (job.note ? ` · ${job.note}` : "");
  }

  function barWidth(job: Job) {
    if (job.status === "done" || job.status === "error") return "100%";
    if (job.total) return `${Math.min(100, (100 * job.done) / job.total)}%`;
    if (job.steps) return `${(100 * Math.max(0, job.step - 1)) / job.steps}%`;
    return job.status === "running" ? "100%" : "0%";
  }
</script>

<svelte:window onpointermove={pointerMove} onpointerup={pointerUp} onpointercancel={pointerUp} />

<div class="transfers" class:dragging={drag}>
  <section class="pane">
    <h3 class="sign">
      <span>Samplers</span>
      <span class="tools">
        <span class="add">
          <button onclick={openAdd} disabled={app.connecting} title="Connect a USB sampler or a simulator" aria-expanded={addOpen}>
            {app.connecting ? "Connecting…" : "+ Add"}
          </button>
          {#if addOpen}
            <div class="menu" role="menu">
              <button role="menuitem" onclick={() => add({ kind: "simulator" })}>Simulator</button>
              {#if usbError}
                <p class="note error">{usbError}</p>
              {:else if !usb}
                <p class="note">Looking for USB samplers…</p>
              {:else if !usb.length}
                <p class="note">No S5000/S6000 on USB</p>
              {:else}
                {#each usb as d (d.key)}
                  <button role="menuitem" title={d.label} disabled={d.connected !== null} onclick={() => add({ kind: "usb", key: d.key })}>
                    {usbLabel(d)}{d.connected !== null ? " · connected" : ""}
                  </button>
                {/each}
              {/if}
            </div>
          {/if}
        </span>
        <button onclick={newProgram} disabled={!ctxConnected}>New program</button>
        <button onclick={newMulti} disabled={!ctxConnected}>New multi</button>
        <button onclick={newFolder} disabled={!ctxFolder}>New folder</button>
        <button onclick={rename} disabled={selectedRows.length !== 1 || selectedRows[0].type !== "item" || busy}>Rename</button>
        <button onclick={remove} disabled={!selectedRows.some((r) => r.type !== "disk") || busy}>Delete</button>
        <button onclick={refresh} disabled={!app.samplers.length} title="Reload memory and open disk folders" aria-label="Reload">↻</button>
      </span>
    </h3>
    <SamplerTree
      {rows}
      selected={sel}
      {focus}
      {target}
      {playing}
      onrowdown={rowDown}
      onrowclick={clickRow}
      onrowdbl={dblRow}
      onplay={play}
      onkeydown={treeKey}
    />
  </section>

  <div class="middle">
    <button onclick={getSelected} disabled={!selectedItems.length || !folder} title="Copy the selected items to the Mac folder">
      Get →
    </button>
    <button
      onclick={sendSelected}
      disabled={!selectedFiles.length || !sendTo}
      title={sendTo ? `Send the selected files to ${app.label(sendTo.id)}` : "Connect a sampler first"}
    >
      ← Send
    </button>
    <hr class="rule soft" />
    <label title="Copy a program's samples, or a multi's programs and samples, with it. You choose which in a dialog.">
      <input type="checkbox" checked={app.settings.deps} onchange={(e) => app.save({ deps: e.currentTarget.checked })} />
      Include dependencies
    </label>
    <label title="Overwrite items with the same name at the destination">
      <input type="checkbox" checked={app.settings.replace} onchange={(e) => app.save({ replace: e.currentTarget.checked })} />
      Replace existing
    </label>
    {#if transfers.dontAsk}
      <p class="hint">
        Copying what is missing{transfers.sessionReplace ? " and replacing" : ""} without asking.
        <button class="link" onclick={() => (transfers.dontAsk = false)}>Ask again</button>
      </p>
    {/if}
    <p class="hint">Files go where their type belongs: .wav to Samples, .akp to Programs, .akm to Multis.</p>
  </div>

  <section class="pane mac" data-drop={macPlace ? JSON.stringify(macPlace) : undefined} class:target={target?.at === "mac"}>
    <h3 class="sign">
      <span>Mac folder</span>
      <span class="tools">
        <button onclick={up} disabled={!folder || folder === "/"} title="Enclosing folder" aria-label="Enclosing folder">↑</button>
        <button onclick={chooseFolder}>Choose…</button>
        <button onclick={loadFolder} disabled={!folder} title="Reload" aria-label="Reload">↻</button>
      </span>
    </h3>
    <div class="path" title={folder}><bdi>{folder || "No folder chosen"}</bdi></div>
    <div class="list" role="listbox" aria-multiselectable="true" tabindex="0" onkeydown={macKey}>
      {#if !folder}
        <p class="empty">Choose a folder, or drop one here from the Finder.</p>
      {:else}
        {#each entries as entry (entry.path)}
          {@const sub = entry.kind === "dir" ? JSON.stringify({ at: "mac", folder: entry.path }) : undefined}
          <!-- svelte-ignore a11y_click_events_have_key_events (the listbox handles keys) -->
          <div
            class="row"
            role="option"
            tabindex="-1"
            aria-selected={macSel.has(entry.path)}
            class:selected={macSel.has(entry.path)}
            class:dir={entry.kind === "dir"}
            class:target={sub !== undefined && target !== null && JSON.stringify(target) === sub}
            data-drop={sub}
            onclick={(e) => clickMac(e, entry)}
            ondblclick={() => dblMac(entry)}
            onpointerdown={(e) => macDown(e, entry)}
          >
            <span class="icon">{entry.kind === "dir" ? "▸" : entry.kind === "sample" ? "∿" : entry.kind === "program" ? "P" : "M"}</span>
            <span class="name">{entry.name}</span>
            <span class="size">{entry.kind === "dir" ? "" : formatBytes(entry.size)}</span>
          </div>
        {:else}
          <p class="empty">No .wav, .akp or .akm files here.</p>
        {/each}
      {/if}
    </div>
    <div class="finder" data-finder class:on={drag?.out} class:show={drag && drag.from === "tree"}>
      {#if drag?.from === "tree" && drag.finder === "fetching"}
        {finderWaiting ? "Waiting for the sampler to finish its other transfers" : `Fetching for the Finder${finderPct === null ? "…" : ` ${finderPct}%`}`} · keep holding
      {:else if drag?.from === "tree"}
        {promiseDrags || transfers.ready(drag.items) ? "Drag out to the Finder" : "Hold here or drag out of the window to fetch for the Finder"}
      {:else}
        Drag items out of the window to copy them to the Finder
      {/if}
    </div>
  </section>
</div>

<section class="queue">
  <h3 class="sign">
    <span>Queue</span>
    {#if queue.pending}<span class="muted-sign">{queue.pending} pending</span>{/if}
    <span class="tools">
      <button onclick={() => queue.clearFinished()} disabled={queue.jobs.length === queue.pending}>Clear finished</button>
    </span>
  </h3>
  <div class="jobs">
    {#each queue.jobs as job (job.id)}
      <div class="job {job.status}">
        <span class="route" title="{job.from} → {job.to}">{job.from} → {job.to}</span>
        <span class="name" title={job.label}>{job.label}</span>
        <span class="bar">
          <span style:width={barWidth(job)} class:indeterminate={job.status === "running" && !job.total && !job.steps}></span>
        </span>
        <span class="state" title={job.note}>
          {jobState(job)}
          {#if job.status === "queued"}
            <button class="x" onclick={() => queue.cancel(job.id)} title="Remove from the queue" aria-label="Remove">✕</button>
          {/if}
        </span>
      </div>
    {:else}
      <p class="empty">
        Drag items between samplers, disks and the Mac folder, or out to the Finder; double-click them, or use Get and Send.
      </p>
    {/each}
  </div>
</section>

{#if drag}
  <div class="ghost" style:left="{drag.x + 12}px" style:top="{drag.y + 8}px">
    {drag.label}
    {#if drag.out}
      <b>→ Finder{drag.finder !== "fetching" ? "" : finderWaiting ? " · waiting" : finderPct === null ? " · fetching…" : ` · fetching ${finderPct}%`}</b>
    {:else if target}
      <b>→ {describePlace(target)}</b>
    {/if}
  </div>
{/if}

<DepDialog />

<style>
  .transfers {
    display: grid;
    grid-template-columns: minmax(0, 1.15fr) 150px minmax(0, 1fr);
    gap: 12px;
    padding: 12px 12px 0;
    height: calc(100% - 196px);
    min-height: 300px;
  }
  .dragging {
    cursor: grabbing;
  }
  .pane,
  .queue {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--surface);
    border-bottom: 1px solid var(--rule-soft);
  }
  .pane.target {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  h3 {
    margin: 0;
  }
  .tools {
    margin-left: auto;
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 4px;
  }
  .sign button {
    background: transparent;
    color: var(--sign-text);
    border-color: var(--sign-muted);
    padding: 0 7px;
    font-size: 11px;
  }
  .sign button:hover:not(:disabled) {
    background: transparent;
    border-color: var(--sign-text);
  }
  .sign button:active:not(:disabled) {
    background: var(--sign-text);
    color: var(--sign-bg);
  }
  .add {
    position: relative;
  }
  .menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 5;
    min-width: 240px;
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--rule);
    display: flex;
    flex-direction: column;
    font-weight: 400;
  }
  .sign .menu button {
    color: var(--text);
    border: none;
    border-bottom: 1px solid var(--rule-soft);
    text-align: left;
    padding: 4px 8px;
    font-size: 12px;
  }
  .sign .menu button:hover:not(:disabled) {
    background: var(--surface-2);
  }
  .menu .note {
    margin: 0;
    padding: 4px 8px;
    font-size: 12px;
    color: var(--muted);
  }
  .menu .note.error {
    color: var(--bad);
  }
  .path {
    padding: 4px 8px;
    font-size: 11px;
    color: var(--muted);
    border-bottom: 1px solid var(--rule);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
  }
  .list {
    flex: 1;
    overflow: auto;
    outline: none;
  }
  .row {
    display: grid;
    grid-template-columns: 16px 1fr 70px;
    gap: 8px;
    align-items: center;
    padding: 0 8px;
    min-height: 23px;
    border-bottom: 1px solid var(--rule-soft);
  }
  .row.selected {
    background: var(--select);
    color: var(--select-text);
  }
  .list:not(:focus-within) .row.selected {
    background: var(--select-soft);
    color: var(--text);
  }
  .row.target {
    background: var(--select-soft);
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .size {
    font-size: 11px;
    color: var(--muted);
    text-align: right;
    white-space: nowrap;
  }
  .selected .size,
  .selected .icon {
    color: inherit;
  }
  .icon {
    font-size: 11px;
    color: var(--muted);
    text-align: center;
  }
  .empty {
    color: var(--muted);
    font-size: 12px;
    padding: 16px;
    margin: 0;
  }
  .finder {
    border-top: 1px solid var(--rule-soft);
    padding: 6px 8px;
    font-size: 11px;
    color: var(--muted);
  }
  .finder.show {
    border: 1px dashed var(--rule);
    color: var(--text);
    font-weight: 700;
    padding: 12px 8px;
  }
  .finder.on {
    background: var(--accent);
    color: var(--accent-text);
    border-color: var(--accent);
  }
  .middle {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 8px;
  }
  .middle button {
    padding: 6px;
  }
  .middle label {
    font-size: 12px;
    font-weight: 700;
  }
  .hint {
    margin: 0;
    font-size: 11px;
    color: var(--muted);
  }
  .link {
    border: none;
    background: none;
    padding: 0;
    font-size: 11px;
    text-decoration: underline;
    color: var(--text);
  }
  .queue {
    margin: 12px;
    height: 160px;
  }
  .muted-sign {
    color: var(--sign-muted);
    font-weight: 400;
  }
  .jobs {
    overflow: auto;
    flex: 1;
  }
  .job {
    display: grid;
    grid-template-columns: minmax(140px, 1.3fr) minmax(110px, 1fr) minmax(80px, 1fr) minmax(140px, 1.5fr);
    gap: 12px;
    align-items: center;
    padding: 0 8px;
    min-height: 23px;
    border-bottom: 1px solid var(--rule-soft);
    font-size: 12px;
  }
  .route {
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .bar {
    height: 4px;
    background: var(--surface-2);
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
    background: var(--sign-bg);
    color: var(--sign-text);
    padding: 3px 8px;
    font-size: 12px;
    font-weight: 700;
    z-index: 20;
    white-space: nowrap;
  }
  .ghost b {
    color: var(--sign-muted);
    margin-left: 6px;
  }
</style>
