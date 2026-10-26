// App-wide state: the connected samplers and their memory, settings and the
// status line. Several samplers can be connected; the "active" one is what
// the front panel, meters and program editor use, and what `info`,
// `memory` and `connected` describe.
import { listen } from "@tauri-apps/api/event";
import { api, message, type Info, type Memory, type Mode, type Origin, type SamplerMeta, type Settings } from "./api";

export interface SamplerView extends SamplerMeta {
  memory: Memory;
  /** Reading the memory list. */
  loading: boolean;
}

const empty = (): Memory => ({ samples: [], programs: [], multis: [] });

class AppState {
  samplers = $state<SamplerView[]>([]);
  connecting = $state(false);
  settings = $state<Settings>({ deps: true, replace: false, mode: "simulator" });
  status = $state("Not connected");
  error = $state<string | null>(null);
  /** Bumped when a sampler's memory changes (after a transfer, delete, …). */
  changed = $state(0);

  get active(): SamplerView | undefined {
    return this.samplers.find((s) => s.active);
  }

  /** The active sampler, if connected. */
  get info(): Info | null {
    const a = this.active;
    if (!a || a.status !== "connected") return null;
    return { name: a.name, os_version: a.os_version, wave_free: a.wave_free, cpu_free: a.cpu_free, simulated: a.simulated };
  }

  get memory(): Memory {
    return this.active?.memory ?? empty();
  }

  get connected() {
    return this.info !== null;
  }

  sampler(id: number): SamplerView | undefined {
    return this.samplers.find((s) => s.id === id);
  }

  /** "S5000", or "S5000 · SIM 2" when several are connected. */
  label(id: number): string {
    const s = this.sampler(id);
    if (!s) return "sampler";
    return this.samplers.length > 1 ? `${s.product} · ${s.short_id}` : s.product;
  }

  async init() {
    try {
      const saved = await api.loadSettings();
      if (saved) Object.assign(this.settings, saved);
    } catch (e) {
      this.fail(e);
    }
    await listen<{ id: number; message: string }>("sampler-lost", ({ payload }) => {
      const s = this.sampler(payload.id);
      if (s) {
        s.status = "lost";
        s.memory = empty();
      }
      this.fail(`${this.label(payload.id)}: ${payload.message}`);
    });
    // Samplers still connected from before a reload of the page.
    try {
      for (const m of await api.samplers()) this.track(m);
      for (const s of this.samplers) void this.refresh(s.id);
    } catch {
      // Not running in the app.
    }
  }

  save(patch: Partial<Settings>) {
    Object.assign(this.settings, patch);
    api.saveSettings($state.snapshot(this.settings)).catch((e) => this.fail(e));
  }

  fail(e: unknown) {
    this.error = message(e);
  }

  say(text: string) {
    this.status = text;
    this.error = null;
  }

  private track(meta: SamplerMeta): SamplerView {
    const old = this.sampler(meta.id);
    if (old) {
      Object.assign(old, meta);
      return old;
    }
    this.samplers.push({ ...meta, memory: empty(), loading: false });
    return this.samplers[this.samplers.length - 1];
  }

  /** Mirrors which sampler the backend treats as active. */
  private async sync() {
    const list = await api.samplers();
    for (const s of this.samplers) s.active = list.find((m) => m.id === s.id)?.active ?? false;
  }

  /** Connects another sampler. */
  async add(origin: Origin): Promise<SamplerView | null> {
    this.connecting = true;
    this.error = null;
    try {
      const s = this.track(await api.addSampler(origin));
      this.say(`Connected to ${s.name}`);
      await this.refresh(s.id);
      return s;
    } catch (e) {
      this.fail(e);
      return null;
    } finally {
      this.connecting = false;
    }
  }

  /** Connects a simulator, or the first USB sampler not yet connected, and makes it active. */
  async connect(mode: Mode) {
    this.save({ mode });
    let origin: Origin = { kind: "simulator" };
    if (mode === "usb") {
      try {
        const free = (await api.usbDevices()).find((d) => d.connected === null);
        if (!free) return this.fail("No other Akai S5000/S6000 found on USB.");
        origin = { kind: "usb", key: free.key };
      } catch (e) {
        return this.fail(e);
      }
    }
    const s = await this.add(origin);
    if (s) await this.setActive(s.id);
  }

  async remove(id: number) {
    const name = this.label(id);
    await api.removeSampler(id).catch((e) => this.fail(e));
    this.samplers = this.samplers.filter((s) => s.id !== id);
    await this.sync().catch(() => {});
    this.say(`Disconnected ${name}`);
  }

  /** Disconnects the active sampler. */
  async disconnect() {
    const a = this.active;
    if (a) await this.remove(a.id);
  }

  async setActive(id: number) {
    try {
      await api.setActive(id);
      for (const s of this.samplers) s.active = s.id === id;
    } catch (e) {
      this.fail(e);
    }
  }

  async reconnect(id: number) {
    try {
      this.track(await api.reconnectSampler(id));
      this.say(`Reconnected ${this.label(id)}`);
      await this.refresh(id);
    } catch (e) {
      this.fail(e);
    }
  }

  /** Reads a sampler's memory list (the active one without `id`). */
  async refresh(id?: number) {
    const s = id === undefined ? this.active : this.sampler(id);
    if (!s || s.status !== "connected") return;
    s.loading = true;
    try {
      s.memory = await api.memory(s.id);
      this.track(await api.samplerInfo(s.id));
      this.changed++;
    } catch (e) {
      this.fail(e);
    } finally {
      s.loading = false;
    }
  }
}

export const app = new AppState();
