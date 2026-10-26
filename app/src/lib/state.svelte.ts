// App-wide state: the connection, what is in the sampler's memory, settings
// and the status line.
import { listen } from "@tauri-apps/api/event";
import { api, message, type Info, type Memory, type Mode, type Settings } from "./api";

class AppState {
  info = $state<Info | null>(null);
  memory = $state<Memory>({ samples: [], programs: [], multis: [] });
  connecting = $state(false);
  settings = $state<Settings>({ deps: true, replace: false, mode: "simulator" });
  status = $state("Not connected");
  error = $state<string | null>(null);

  get connected() {
    return this.info !== null;
  }

  async init() {
    try {
      const saved = await api.loadSettings();
      if (saved) Object.assign(this.settings, saved);
    } catch (e) {
      this.fail(e);
    }
    await listen<string>("disconnected", (e) => {
      this.info = null;
      this.memory = { samples: [], programs: [], multis: [] };
      this.fail(e.payload);
    });
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

  async connect(mode: Mode) {
    this.connecting = true;
    this.error = null;
    this.save({ mode });
    try {
      this.info = await api.connect(mode);
      this.say(`Connected to ${this.info.name}`);
      await this.refresh();
    } catch (e) {
      this.info = null;
      this.fail(e);
    } finally {
      this.connecting = false;
    }
  }

  async disconnect() {
    await api.disconnect().catch((e) => this.fail(e));
    this.info = null;
    this.memory = { samples: [], programs: [], multis: [] };
    this.say("Disconnected");
  }

  async refresh() {
    if (!this.connected) return;
    try {
      this.memory = await api.memory();
      this.info = await api.info();
    } catch (e) {
      this.fail(e);
    }
  }
}

export const app = new AppState();
