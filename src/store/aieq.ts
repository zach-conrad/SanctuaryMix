import { create } from "zustand";
import { getBackend, type Backend } from "../lib/backend";
import { allows } from "../lib/plans";
import type {
  AiEqConfig,
  AiEqStatus,
  ChannelEq,
  CompareSide,
  EqAudit,
  EqChannelStatus,
  EqIdea,
  EqLogEntry,
  IdeaState,
} from "../lib/types";
import { useMixer } from "./mixer";

const LOG_KEEP = 200;

export type AssistTab = "levels" | "eq";

export interface EqReport {
  recordingId: string;
  ideas: EqIdea[];
  audit: EqAudit;
}

interface AiEqState {
  ready: boolean;
  config: AiEqConfig | null;
  status: AiEqStatus | null;
  /** Newest first. */
  log: EqLogEntry[];
  error: string | null;
  /** The wide EQ panel is open in place of the Inspector, for the selected channel. */
  panelOpen: boolean;
  /** Which half of the Assist page shows. */
  assistTab: AssistTab;
  /** When each channel's soundcheck change was applied from this app (for "Applied" and Before | After). */
  applied: Record<number, number>;
  /** The EQ part of the open service's report. */
  report: EqReport | null;

  init(): Promise<void>;
  /** Opens the EQ panel over the Mixer for a channel. */
  openPanel(channel: number): void;
  closePanel(): void;
  setAssistTab(tab: AssistTab): void;
  clearError(): void;

  setEnabled(on: boolean): Promise<void>;
  setToneKeeping(on: boolean): Promise<void>;
  soundcheckStart(): Promise<void>;
  soundcheckStop(): Promise<void>;
  apply(channel: number): Promise<void>;
  applyAll(): Promise<void>;
  skip(channel: number): Promise<void>;
  keepMyEq(): Promise<void>;
  undo(channel: number): Promise<void>;
  undoAll(): Promise<void>;
  handBack(channel: number): Promise<void>;
  setEq(channel: number, eq: ChannelEq): Promise<void>;
  compare(channel: number, side: CompareSide | null): Promise<void>;
  restoreProfile(): Promise<void>;
  dismissProfile(): Promise<void>;
  ringOutStart(): Promise<void>;
  ringOutStop(): Promise<void>;
  dismissFeedback(): Promise<void>;
  loadReport(recordingId: string): Promise<void>;
  setIdea(id: string, state: IdeaState): Promise<void>;
}

function reportError(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}

let initialized = false;

export const useAiEq = create<AiEqState>((set, get) => {
  /** Runs one backend call; a refusal from the core shows as the store's error. */
  async function run(action: (b: Backend) => Promise<void>): Promise<boolean> {
    try {
      await action(await getBackend());
      set({ error: null });
      return true;
    } catch (e) {
      set({ error: reportError(e) });
      return false;
    }
  }

  /** Sends new settings to the core and keeps what it actually applied. */
  async function save(next: AiEqConfig) {
    const before = get().config;
    set({ config: next, error: null });
    try {
      set({ config: await (await getBackend()).aieqSetConfig(next) });
    } catch (e) {
      set({ config: before, error: reportError(e) });
    }
  }

  async function refresh(b: Backend) {
    set({ status: await b.aieqStatus() });
  }

  return {
    ready: false,
    config: null,
    status: null,
    log: [],
    error: null,
    panelOpen: false,
    assistTab: "levels",
    applied: {},
    report: null,

    async init() {
      if (initialized) return;
      initialized = true;
      const backend = await getBackend();
      await backend.onAiEq((status) => set({ status }));
      await backend.onAiEqLog((entry) => set((s) => ({ log: [entry, ...s.log].slice(0, LOG_KEEP) })));
      const [config, status, log] = await Promise.all([
        backend.aieqGetConfig(),
        backend.aieqStatus(),
        backend.aieqLog(LOG_KEEP),
      ]);
      set({ config, status, log, ready: true });
    },

    openPanel(channel) {
      useMixer.getState().select(channel);
      set({ panelOpen: true });
    },
    closePanel: () => set({ panelOpen: false }),
    setAssistTab: (assistTab) => set({ assistTab }),
    clearError: () => set({ error: null }),

    async setEnabled(on) {
      const config = get().config;
      if (config) await save({ ...config, enabled: on });
    },
    async setToneKeeping(on) {
      const config = get().config;
      if (config) await save({ ...config, toneKeeping: on });
    },

    soundcheckStart: () => run((b) => b.aieqSoundcheckStart()).then(() => undefined),
    soundcheckStop: () => run((b) => b.aieqSoundcheckStop()).then(() => undefined),
    async apply(channel) {
      if (await run((b) => b.aieqApply(channel))) set((s) => ({ applied: { ...s.applied, [channel]: Date.now() } }));
    },
    async applyAll() {
      const waiting = (get().status?.channels ?? []).filter((c) => c.proposal).map((c) => c.channel);
      if (await run((b) => b.aieqApplyAll())) {
        const at = Date.now();
        set((s) => ({ applied: { ...s.applied, ...Object.fromEntries(waiting.map((c) => [c, at])) } }));
      }
    },
    skip: (channel) => run((b) => b.aieqSkip(channel)).then(() => undefined),
    keepMyEq: () => run((b) => b.aieqKeepMyEq()).then(() => undefined),
    async undo(channel) {
      if (await run((b) => b.aieqUndo(channel))) {
        set((s) => {
          const applied = { ...s.applied };
          delete applied[channel];
          return { applied };
        });
      }
    },
    undoAll: () => run((b) => b.aieqUndoAll()).then(() => undefined),
    handBack: (channel) => run((b) => b.aieqHandBack(channel)).then(() => undefined),
    setEq: (channel, eq) => run((b) => b.aieqSetEq(channel, eq)).then(() => undefined),
    compare: (channel, side) => run((b) => b.aieqCompare(channel, side)).then(() => undefined),
    restoreProfile: () => run((b) => b.aieqRestoreProfile()).then(() => undefined),
    dismissProfile: () => run((b) => b.aieqDismissProfile()).then(() => undefined),
    ringOutStart: () => run((b) => b.aieqRingOutStart().then(() => refresh(b))).then(() => undefined),
    ringOutStop: () => run((b) => b.aieqRingOutStop()).then(() => undefined),
    dismissFeedback: () => run((b) => b.aieqDismissFeedback()).then(() => undefined),

    async loadReport(recordingId) {
      if (get().report?.recordingId !== recordingId) set({ report: null });
      try {
        const b = await getBackend();
        const [ideas, audit] = await Promise.all([b.aieqIdeas(recordingId), b.aieqAudit(recordingId)]);
        set({ report: { recordingId, ideas, audit } });
      } catch (e) {
        set({ error: reportError(e) });
      }
    },
    async setIdea(id, state) {
      try {
        const ideas = await (await getBackend()).aieqSetIdea(id, state);
        set((s) => {
          if (!s.report) return {};
          const byId = new Map(ideas.map((i) => [i.id, i]));
          return { report: { ...s.report, ideas: s.report.ideas.map((i) => byId.get(i.id) ?? i) }, error: null };
        });
      } catch (e) {
        set({ error: reportError(e) });
      }
    },
  };
});

/** AI EQ status for one picked channel. */
export function useEqChannel(channel: number): EqChannelStatus | undefined {
  return useAiEq((s) => s.status?.channels.find((c) => c.channel === channel));
}

/** AI EQ is in the plan and switched on. */
export function useEqActive(): boolean {
  const allowed = useMixer((s) => allows(s.session, "aiEq"));
  const enabled = useAiEq((s) => s.status?.enabled ?? false);
  return allowed && enabled;
}

/** Why AI EQ can't run, as one line for the panel, or null when it can. */
export function eqBlocker(lock: string | null): string | null {
  if (!lock) return null;
  if (lock === "Sign in to use") return "Sign in to use AI EQ.";
  if (lock.startsWith("Available on")) return "AI EQ is in Pro. See Settings.";
  return `${lock}. See Settings.`;
}
