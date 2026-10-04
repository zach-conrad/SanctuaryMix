import { create } from "zustand";
import { getBackend } from "../lib/backend";
import type {
  Adjustment,
  AutoMixConfig,
  AutoMixStatus,
  ChannelRole,
  ChannelStatus,
  Nudges,
  Preset,
  RoomFeel,
} from "../lib/types";
import { useMixer } from "./mixer";

const LOG_KEEP = 200;

interface AutoMixState {
  ready: boolean;
  presets: Preset[];
  config: AutoMixConfig | null;
  status: AutoMixStatus | null;
  /** Newest first. */
  log: Adjustment[];
  error: string | null;

  init(): Promise<void>;
  setFeel(feel: RoomFeel): Promise<void>;
  setNudge(key: keyof Nudges, db: number): Promise<void>;
  /** Hands channels to auto-mix (role guessed from the console name) or takes them back. */
  setManaged(channels: number[], on: boolean): Promise<void>;
  setRole(channel: number, role: ChannelRole): Promise<void>;
  engage(on: boolean): Promise<void>;
  freeze(): Promise<void>;
  resume(): Promise<void>;
  resumeChannel(channel: number): Promise<void>;
  undo(channel: number): Promise<void>;
  undoAll(): Promise<void>;
}

function reportError(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}

let initialized = false;

export const useAutoMix = create<AutoMixState>((set, get) => {
  /** Sends new settings to the core and keeps what it actually applied. */
  async function save(next: AutoMixConfig) {
    const before = get().config;
    set({ config: next, error: null });
    try {
      const applied = await (await getBackend()).automixSetConfig(next);
      set({ config: applied });
    } catch (e) {
      set({ config: before, error: reportError(e) });
    }
  }

  async function run(action: (b: Awaited<ReturnType<typeof getBackend>>) => Promise<void>) {
    try {
      await action(await getBackend());
      set({ error: null });
    } catch (e) {
      set({ error: reportError(e) });
    }
  }

  return {
    ready: false,
    presets: [],
    config: null,
    status: null,
    log: [],
    error: null,

    async init() {
      if (initialized) return;
      initialized = true;
      const backend = await getBackend();
      await backend.onAutomix((status) => set({ status }));
      await backend.onAutomixAdjustment((a) => set((s) => ({ log: [a, ...s.log].slice(0, LOG_KEEP) })));
      const [presets, config, status, log] = await Promise.all([
        backend.automixPresets(),
        backend.automixGetConfig(),
        backend.automixStatus(),
        backend.automixLog(LOG_KEEP),
      ]);
      set({ presets, config, status, log, ready: true });
      // The core starts with the saved settings; make sure it has them.
      await save(config);
    },

    async setFeel(feel) {
      const config = get().config;
      if (config) await save({ ...config, feel });
    },

    async setNudge(key, db) {
      const config = get().config;
      if (config) await save({ ...config, nudges: { ...config.nudges, [key]: db } });
    },

    async setManaged(channels, on) {
      const config = get().config;
      if (!config) return;
      const kept = config.channels.filter((c) => !channels.includes(c.channel));
      if (!on) {
        await save({ ...config, channels: kept });
        return;
      }
      const strips = useMixer.getState().strips;
      const roles = await (await getBackend()).automixGuessRoles(channels.map((ch) => strips[ch]?.name ?? ""));
      const existing = new Map(config.channels.map((c) => [c.channel, c.role]));
      const added = channels.map((channel, i) => ({ channel, role: existing.get(channel) ?? roles[i] }));
      await save({ ...config, channels: [...kept, ...added].sort((a, b) => a.channel - b.channel) });
    },

    async setRole(channel, role) {
      const config = get().config;
      if (!config) return;
      await save({ ...config, channels: config.channels.map((c) => (c.channel === channel ? { ...c, role } : c)) });
    },

    engage: (on) => run((b) => b.automixEngage(on)),
    freeze: () => run((b) => b.automixFreeze()),
    resume: () => run((b) => b.automixResume()),
    resumeChannel: (channel) => run((b) => b.automixResumeChannel(channel)),
    undo: (channel) => run((b) => b.automixUndo(channel)),
    undoAll: () => run((b) => b.automixUndoAll()),
  };
});

/** Auto-mix status for one channel, if it's handed over. */
export function useChannelAuto(channel: number): ChannelStatus | undefined {
  return useAutoMix((s) => s.status?.channels.find((c) => c.channel === channel));
}

/** True when auto-mix moved this fader away from where the operator left it. */
export function isAutoChanged(c: ChannelStatus | undefined): boolean {
  if (!c || c.mode === "heldByOperator" || c.mode === "off") return false;
  return c.faderDb !== null && c.baselineDb !== null && Math.abs(c.faderDb - c.baselineDb) >= 0.25;
}

export const ROLE_LABEL: Record<ChannelRole, string> = {
  speech: "Speech",
  leadVocal: "Lead vocal",
  backingVocal: "Backing vocal",
  choir: "Choir",
  kick: "Kick",
  bass: "Bass",
  drums: "Drums",
  keysPads: "Keys and pads",
  pianoOrgan: "Piano or organ",
  electricGuitar: "Electric guitar",
  acousticGuitar: "Acoustic guitar",
  playback: "Tracks or playback",
  other: "Other",
};

export const MODE_LABEL: Record<ChannelStatus["mode"], string> = {
  off: "Off",
  consoleOffline: "Console offline",
  frozen: "Frozen",
  heldByOperator: "Yours",
  undone: "Undone",
  waitingForFader: "Needs a nudge",
  muted: "Muted",
  clipping: "Clipping",
  noAudio: "No audio",
  idle: "Not in use",
  waitingForLead: "Waiting for vocals",
  riding: "Riding",
  atLimit: "At its limit",
  settled: "On target",
};

/** One sentence for the inspector and tooltips. */
export const MODE_DETAIL: Record<ChannelStatus["mode"], string> = {
  off: "Auto-mix is off.",
  consoleOffline: "The console isn't connected, so nothing moves.",
  frozen: "Everything is frozen. Resume when you're ready.",
  heldByOperator: "You moved this fader, so auto-mix let go of it. Hand it back when you're ready.",
  undone: "Back where you set it. Auto-mix leaves it alone until you hand it back.",
  waitingForFader: "Auto-mix doesn't know where this fader is yet. Nudge it on the desk once.",
  muted: "Muted, or just unmuted. Auto-mix never mutes or unmutes and waits 5 seconds after an unmute.",
  clipping: "The input is clipping. A fader can't fix that; turn the preamp gain down on the console.",
  noAudio: "No Dante audio is arriving, so nothing moves.",
  idle: "Nothing is coming through this mic, so auto-mix won't raise it.",
  waitingForLead: "Holding the band where it is until a lead vocal is singing.",
  riding: "Moving gently toward its place in the mix.",
  atLimit: "Wants to go further but is at the edge of its range. If this keeps happening, check its preamp gain.",
  settled: "Sitting where it should for this room feel.",
};
