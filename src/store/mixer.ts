import { create } from "zustand";
import { getBackend } from "../lib/backend";
import type { ChannelId, ConsoleConfig, ConsoleEvent, MeterFrame, MeteringInfo, Session } from "../lib/types";

export type View = "mixer" | "scenes" | "recordings" | "assist" | "setup" | "settings";
export type Theme = "dark" | "light" | "system";
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
  /** Chose "Mix without signing in" this launch; the sign-in screen stays away. */
  workingLocally: boolean;

  audioStatus: LinkStatus;
  audio: MeteringInfo | null;
  audioError: string | null;

  consoleStatus: LinkStatus;
  consoleModel: string | null;
  consoleError: string | null;
  consoleConfig: ConsoleConfig;

  strips: Strip[];
  selected: number;
  /** Mute changes sent but not yet confirmed by the console: index → requested state. */
  pendingMutes: Record<number, boolean>;
  /** Set when the console hasn't confirmed a change within 2 seconds. */
  unconfirmed: boolean;
  theme: Theme;

  setView(view: View): void;
  select(index: number): void;
  setTheme(theme: Theme): void;
  init(): Promise<void>;
  /** Rejects with a sentence to show under the form. */
  signIn(email: string, password: string): Promise<void>;
  /** Opens Google sign-in in the browser; the session arrives by event. */
  signInWithGoogle(): Promise<void>;
  /** Why Google sign-in didn't finish, for the sign-in screen. */
  signInError: string | null;
  /** Rejects with a sentence to show (refused mid-service). */
  signOut(): Promise<void>;
  workLocally(): void;
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

const THEME_KEY = "sanctuarymix.theme";

function loadTheme(): Theme {
  let theme: Theme = "dark";
  try {
    const saved = localStorage.getItem(THEME_KEY);
    if (saved === "light" || saved === "system" || saved === "dark") theme = saved;
  } catch {
    // Fall back to the default dark theme.
  }
  applyTheme(theme);
  return theme;
}

function applyTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme;
}

export const useMixer = create<MixerState>((set, get) => ({
  view: "mixer",
  isDemo: false,
  session: null,
  signInError: null,
  workingLocally: false,
  audioStatus: "off",
  audio: null,
  audioError: null,
  consoleStatus: "off",
  consoleModel: null,
  consoleError: null,
  consoleConfig: DEFAULT_CONFIG,
  strips: makeStrips(DEFAULT_CONFIG.inputCount),
  selected: 0,
  pendingMutes: {},
  unconfirmed: false,
  theme: loadTheme(),

  setView: (view) => set({ view }),
  select: (selected) => set({ selected }),
  setTheme(theme) {
    applyTheme(theme);
    try {
      localStorage.setItem(THEME_KEY, theme);
    } catch {
      // Storage can be unavailable; the theme still applies for this run.
    }
    set({ theme });
  },

  async init() {
    if (initialized) return;
    initialized = true;
    const backend = await getBackend();
    set({ isDemo: backend.isDemo, session: await backend.getSession() });
    await backend.onSession((session) => set({ session, signInError: null }));
    await backend.onSessionError((signInError) => set({ signInError }));
    await backend.onMeters((frame) => useMeters.setState({ frame }));
    await backend.onConsole((event) => applyConsoleEvent(event, set, get));
    // In the browser demo, start with something to look at.
    if (backend.isDemo) {
      await get().startAudio("Dante Virtual Soundcard");
      await get().connectConsole(DEFAULT_CONFIG);
    }
  },

  async signIn(email, password) {
    const backend = await getBackend();
    try {
      set({ session: await backend.signInWithPassword(email, password), workingLocally: false, signInError: null });
    } catch (e) {
      throw reportError(e);
    }
  },

  async signInWithGoogle() {
    set({ signInError: null });
    try {
      await (await getBackend()).beginGoogleSignIn();
    } catch (e) {
      throw reportError(e);
    }
  },

  async signOut() {
    const backend = await getBackend();
    try {
      set({ session: await backend.signOut(), workingLocally: false });
    } catch (e) {
      throw reportError(e);
    }
  },

  workLocally: () => set({ workingLocally: true }),

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
      const detail = reportError(e);
      set({
        consoleStatus: "error",
        consoleError:
          config.model === "dlive"
            ? `Can't reach the dLive at ${config.host}. Check the network cable and the IP address, then try again. (${detail})`
            : detail,
      });
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
    // The key keeps showing the console's state until the console confirms.
    const { strips, pendingMutes } = get();
    const muted = !(pendingMutes[index] ?? strips[index]?.muted);
    set({ pendingMutes: { ...pendingMutes, [index]: muted } });
    setTimeout(() => {
      if (get().pendingMutes[index] !== undefined) set({ unconfirmed: true });
    }, 2000);
    void getBackend()
      .then((b) => b.setMute(input(index), muted))
      .catch((e) => {
        clearPending(set, index);
        set({ consoleError: reportError(e) });
      });
  },
}));

type Set = (partial: Partial<MixerState> | ((s: MixerState) => Partial<MixerState>)) => void;

function clearPending(set: Set, index: number) {
  set((s) => {
    const pendingMutes = { ...s.pendingMutes };
    delete pendingMutes[index];
    return { pendingMutes, unconfirmed: Object.keys(pendingMutes).length > 0 && s.unconfirmed };
  });
}

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
        consoleError: event.reason ? `Lost the console: ${event.reason}. Check the network, then connect again.` : null,
        consoleModel: null,
        pendingMutes: {},
        unconfirmed: false,
      });
      break;
    case "fader":
      if (event.id.kind === "input") updateStrip(set, event.id.index, { faderDb: event.db });
      break;
    case "mute":
      if (event.id.kind === "input") {
        updateStrip(set, event.id.index, { muted: event.muted });
        clearPending(set, event.id.index);
      }
      break;
    case "name":
      if (event.id.kind === "input" && event.id.index < get().strips.length) {
        updateStrip(set, event.id.index, { name: event.name || `Ch ${event.id.index + 1}` });
      }
      break;
  }
}
