import { create } from "zustand";
import { getBackend } from "../lib/backend";
import type { ChannelId, ConsoleConfig, ConsoleEvent, MeterFrame, MeteringInfo, Session } from "../lib/types";

export type View = "mix" | "assistant" | "setup" | "settings";
type LinkStatus = "off" | "connecting" | "on" | "error";

export interface Strip {
  index: number;
  name: string;
  faderDb: number | null;
  muted: boolean;
}

interface MixerState {
  view: View;
  isDemo: boolean;
  session: Session | null;

  audioStatus: LinkStatus;
  audio: MeteringInfo | null;
  audioError: string | null;

  consoleStatus: LinkStatus;
  consoleModel: string | null;
  consoleError: string | null;
  consoleConfig: ConsoleConfig;

  strips: Strip[];

  setView(view: View): void;
  init(): Promise<void>;
  startAudio(device: string | null): Promise<void>;
  stopAudio(): Promise<void>;
  connectConsole(config: ConsoleConfig): Promise<void>;
  disconnectConsole(): Promise<void>;
  setFader(index: number, db: number | null): void;
  toggleMute(index: number): void;
}

const DEFAULT_CONFIG: ConsoleConfig = {
  model: "simulated",
  host: "192.168.1.70",
  port: null,
  midiChannel: 0,
  inputCount: 32,
};

const input = (index: number): ChannelId => ({ kind: "input", index });

function makeStrips(count: number, existing: Strip[] = []): Strip[] {
  return Array.from(
    { length: count },
    (_, i) => existing[i] ?? { index: i, name: `Ch ${i + 1}`, faderDb: 0, muted: false },
  );
}

function reportError(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}

/**
 * Meters update ~30x a second, so they live in their own tiny store that
 * components subscribe to per channel, keeping the rest of the UI still.
 */
export const useMeters = create<{ frame: MeterFrame | null }>(() => ({ frame: null }));

let initialized = false;

export const useMixer = create<MixerState>((set, get) => ({
  view: "mix",
  isDemo: false,
  session: null,
  audioStatus: "off",
  audio: null,
  audioError: null,
  consoleStatus: "off",
  consoleModel: null,
  consoleError: null,
  consoleConfig: DEFAULT_CONFIG,
  strips: makeStrips(DEFAULT_CONFIG.inputCount),

  setView: (view) => set({ view }),

  async init() {
    if (initialized) return;
    initialized = true;
    const backend = await getBackend();
    set({ isDemo: backend.isDemo, session: await backend.getSession() });
    await backend.onMeters((frame) => useMeters.setState({ frame }));
    await backend.onConsole((event) => applyConsoleEvent(event, set, get));
    // In the browser demo, start with something to look at.
    if (backend.isDemo) {
      await get().startAudio("Dante Virtual Soundcard");
      await get().connectConsole(DEFAULT_CONFIG);
    }
  },

  async startAudio(device) {
    set({ audioStatus: "connecting", audioError: null });
    try {
      const audio = await (await getBackend()).startMetering(device);
      set({ audioStatus: "on", audio });
    } catch (e) {
      set({ audioStatus: "error", audioError: reportError(e), audio: null });
    }
  },

  async stopAudio() {
    await (await getBackend()).stopMetering();
    useMeters.setState({ frame: null });
    set({ audioStatus: "off", audio: null });
  },

  async connectConsole(config) {
    set({
      consoleStatus: "connecting",
      consoleError: null,
      consoleConfig: config,
      strips: makeStrips(config.inputCount, get().strips),
    });
    try {
      await (await getBackend()).connectConsole(config);
      set({ consoleStatus: "on" });
    } catch (e) {
      set({ consoleStatus: "error", consoleError: reportError(e) });
    }
  },

  async disconnectConsole() {
    await (await getBackend()).disconnectConsole();
    set({ consoleStatus: "off", consoleModel: null });
  },

  setFader(index, db) {
    updateStrip(set, index, { faderDb: db });
    void getBackend()
      .then((b) => b.setFader(input(index), db))
      .catch((e) => set({ consoleError: reportError(e) }));
  },

  toggleMute(index) {
    const muted = !get().strips[index]?.muted;
    updateStrip(set, index, { muted });
    void getBackend()
      .then((b) => b.setMute(input(index), muted))
      .catch((e) => set({ consoleError: reportError(e) }));
  },
}));

type Set = (partial: Partial<MixerState> | ((s: MixerState) => Partial<MixerState>)) => void;

function updateStrip(set: Set, index: number, patch: Partial<Strip>) {
  set((s) => ({ strips: s.strips.map((st) => (st.index === index ? { ...st, ...patch } : st)) }));
}

function applyConsoleEvent(event: ConsoleEvent, set: Set, get: () => MixerState) {
  switch (event.type) {
    case "connected":
      set({ consoleStatus: "on", consoleModel: event.model });
      break;
    case "disconnected":
      set({
        consoleStatus: event.reason ? "error" : "off",
        consoleError: event.reason,
        consoleModel: null,
      });
      break;
    case "fader":
      if (event.id.kind === "input") updateStrip(set, event.id.index, { faderDb: event.db });
      break;
    case "mute":
      if (event.id.kind === "input") updateStrip(set, event.id.index, { muted: event.muted });
      break;
    case "name":
      if (event.id.kind === "input" && event.id.index < get().strips.length) {
        updateStrip(set, event.id.index, { name: event.name || `Ch ${event.id.index + 1}` });
      }
      break;
  }
}
