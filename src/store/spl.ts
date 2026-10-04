import { create } from "zustand";
import { getBackend } from "../lib/backend";
import type { SplConfig, SplPoint, SplReading, Weighting } from "../lib/types";

/** Graph spans the popup offers, in minutes. */
export const SPANS = [5, 15, 60] as const;
export type Span = (typeof SPANS)[number];

interface SplState {
  ready: boolean;
  config: SplConfig | null;
  reading: SplReading | null;
  history: SplPoint[];
  span: Span;
  error: string | null;

  /** Loads settings and starts listening for readings. Safe to call more than once. */
  init(): Promise<void>;
  setSource(source: number | null): Promise<void>;
  setOffset(offsetDb: number): Promise<void>;
  calibrate(weighting: Weighting, referenceDb: number): Promise<boolean>;
  reset(): Promise<void>;
  setSpan(span: Span): void;
  /** Re-reads the graph history for the current span. */
  loadHistory(): Promise<void>;
  clearError(): void;
}

function reportError(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}

let started: Promise<void> | null = null;

export const useSpl = create<SplState>((set, get) => {
  async function saveConfig(config: SplConfig) {
    try {
      const backend = await getBackend();
      const applied = await backend.splSetConfig(config);
      const sourceChanged = applied.source !== get().config?.source;
      set({ config: applied, error: null, ...(sourceChanged ? { reading: null, history: [] } : {}) });
    } catch (e) {
      set({ error: reportError(e) });
    }
  }

  return {
    ready: false,
    config: null,
    reading: null,
    history: [],
    span: 15,
    error: null,

    init() {
      started ??= (async () => {
        const backend = await getBackend();
        const [config, reading] = await Promise.all([backend.splGetConfig(), backend.splReading()]);
        set({ config, reading, ready: true });
        await backend.onSpl((r) => set({ reading: r }));
      })().catch((e: unknown) => {
        started = null;
        set({ error: reportError(e) });
      });
      return started;
    },

    async setSource(source) {
      const config = get().config;
      if (config) await saveConfig({ ...config, source });
    },

    async setOffset(offsetDb) {
      const config = get().config;
      // A typed offset is a guess unless it came from a calibration.
      if (config) await saveConfig({ ...config, offsetDb, calibrated: false });
    },

    async calibrate(weighting, referenceDb) {
      try {
        const backend = await getBackend();
        const config = await backend.splCalibrate(weighting, referenceDb);
        set({ config, error: null });
        await get().loadHistory();
        return true;
      } catch (e) {
        set({ error: reportError(e) });
        return false;
      }
    },

    async reset() {
      const backend = await getBackend();
      await backend.splReset();
      set({ reading: null, history: [] });
    },

    setSpan(span) {
      set({ span });
      void get().loadHistory();
    },

    async loadHistory() {
      try {
        const backend = await getBackend();
        set({ history: await backend.splHistory(get().span * 60) });
      } catch (e) {
        set({ error: reportError(e) });
      }
    },

    clearError() {
      set({ error: null });
    },
  };
});
