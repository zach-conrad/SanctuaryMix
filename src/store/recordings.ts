import { create } from "zustand";
import { getBackend } from "../lib/backend";
import type {
  ChannelId,
  DiskUsage,
  OutputDeviceInfo,
  PlaybackStatus,
  RecordedEvent,
  RecorderStatus,
  RecordingDetail,
  RecordingSettings,
  RecordingSummary,
  ReplayStatus,
} from "../lib/types";
import { useMixer } from "./mixer";

interface RecordingsState {
  list: RecordingSummary[];
  loaded: boolean;
  disk: DiskUsage | null;
  settings: RecordingSettings | null;
  /** Live recorder state; `active` is set while a service is recording. */
  recorder: RecorderStatus | null;
  /** Shown under the top bar after Stop. */
  justSaved: RecordingSummary | null;
  recordBusy: boolean;
  recordError: string | null;
  error: string | null;

  /** The service open in the detail view. */
  openId: string | null;
  detail: RecordingDetail | null;
  events: RecordedEvent[];
  playback: PlaybackStatus | null;
  outputDevices: OutputDeviceInfo[];
  replay: ReplayStatus | null;
  replayError: string | null;

  init(): Promise<void>;
  refresh(): Promise<void>;
  startRecording(): Promise<void>;
  stopRecording(): Promise<void>;
  dismissSaved(): void;
  saveSettings(settings: RecordingSettings): Promise<void>;
  deleteOldMultitracks(days: number): Promise<number>;

  open(id: string): Promise<void>;
  close(): Promise<void>;
  saveDetails(title: string, notes: string): Promise<void>;
  deleteOpen(): Promise<void>;
  play(): Promise<void>;
  pause(): Promise<void>;
  seek(positionMs: number): Promise<void>;
  setOutputDevice(name: string): Promise<void>;
  startReplay(channels: ChannelId[] | null): Promise<void>;
  stopReplay(): Promise<void>;
  restore(): Promise<void>;
  clearReplayMessage(): void;
}

function reportError(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}

let initialized = false;

export const useRecordings = create<RecordingsState>((set, get) => {
  async function attempt(action: () => Promise<void>, field: "error" | "replayError" = "error") {
    try {
      set({ [field]: null } as Partial<RecordingsState>);
      await action();
    } catch (e) {
      set({ [field]: reportError(e) } as Partial<RecordingsState>);
    }
  }

  return {
    list: [],
    loaded: false,
    disk: null,
    settings: null,
    recorder: null,
    justSaved: null,
    recordBusy: false,
    recordError: null,
    error: null,
    openId: null,
    detail: null,
    events: [],
    playback: null,
    outputDevices: [],
    replay: null,
    replayError: null,

    async init() {
      if (initialized) return;
      initialized = true;
      const b = await getBackend();
      await b.onRecorder((recorder) => {
        const wasActive = get().recorder?.active?.id;
        set({ recorder });
        // Keep the list's live row and the disk panel current.
        if (recorder.active || wasActive) {
          set((s) => ({
            list: s.list.map((r) => (r.id === recorder.active?.id ? recorder.active : r)),
            disk: s.disk ? { ...s.disk, freeBytes: recorder.freeBytes } : s.disk,
          }));
        }
      });
      await b.onPlayback((playback) => {
        if (playback.recordingId === get().openId) set({ playback });
      });
      await b.onReplay((replay) => set({ replay }));
      const [recorder, settings, outputDevices] = await Promise.all([
        b.recorderStatus(),
        b.getRecordingSettings(),
        b.listOutputDevices(),
      ]);
      set({ recorder, settings, outputDevices });
      await get().refresh();
    },

    async refresh() {
      await attempt(async () => {
        const b = await getBackend();
        const [list, disk] = await Promise.all([b.listRecordings(), b.diskUsage()]);
        set({ list, disk, loaded: true });
      });
    },

    async startRecording() {
      set({ recordBusy: true, recordError: null, justSaved: null });
      try {
        const b = await getBackend();
        await b.startRecording(null);
        set({ recorder: await b.recorderStatus() });
        await get().refresh();
      } catch (e) {
        set({ recordError: reportError(e) });
      } finally {
        set({ recordBusy: false });
      }
    },

    async stopRecording() {
      set({ recordBusy: true, recordError: null });
      try {
        const b = await getBackend();
        const saved = await b.stopRecording();
        set({ justSaved: saved, recorder: await b.recorderStatus() });
        await get().refresh();
      } catch (e) {
        set({ recordError: reportError(e) });
      } finally {
        set({ recordBusy: false });
      }
    },

    dismissSaved: () => set({ justSaved: null, recordError: null }),

    async saveSettings(settings) {
      const before = get().settings;
      set({ settings });
      try {
        await (await getBackend()).setRecordingSettings(settings);
      } catch (e) {
        set({ settings: before, error: reportError(e) });
      }
    },

    async deleteOldMultitracks(days) {
      let freed = 0;
      await attempt(async () => {
        freed = await (await getBackend()).deleteOldMultitracks(days);
        await get().refresh();
      });
      return freed;
    },

    async open(id) {
      if (get().openId && get().openId !== id) await get().close();
      set({ openId: id, detail: null, events: [], playback: null, error: null });
      useMixer.getState().setView("recordings");
      await attempt(async () => {
        const b = await getBackend();
        const [detail, events] = await Promise.all([b.getRecording(id), b.recordingEvents(id)]);
        if (get().openId !== id) return;
        set({ detail, events });
        const device = get().playback?.device ?? get().outputDevices.find((d) => d.isDefault)?.name ?? null;
        const playback = await b.loadPlayback(id, device);
        if (get().openId === id) set({ playback });
      });
    },

    async close() {
      const replaying = get().replay?.state === "running";
      set({ openId: null, detail: null, events: [], playback: null });
      if (!replaying) await attempt(async () => (await getBackend()).unloadPlayback());
    },

    async saveDetails(title, notes) {
      const id = get().openId;
      if (!id) return;
      await attempt(async () => {
        const updated = await (await getBackend()).updateRecording(id, title, notes);
        set((s) => ({
          detail: s.detail && s.detail.id === id ? { ...s.detail, title: updated.title, notes: updated.notes } : s.detail,
          list: s.list.map((r) => (r.id === id ? updated : r)),
        }));
      });
    },

    async deleteOpen() {
      const id = get().openId;
      if (!id) return;
      await attempt(async () => {
        const b = await getBackend();
        await b.unloadPlayback();
        await b.deleteRecording(id);
        set({ openId: null, detail: null, events: [], playback: null });
        await get().refresh();
      });
    },

    play: () => attempt(async () => (await getBackend()).play()),
    pause: () => attempt(async () => (await getBackend()).pause()),
    async seek(positionMs) {
      const p = get().playback;
      if (p) set({ playback: { ...p, positionMs } });
      await attempt(async () => (await getBackend()).seek(positionMs));
    },
    async setOutputDevice(name) {
      const id = get().openId;
      if (!id) return;
      await attempt(async () => {
        const playback = await (await getBackend()).loadPlayback(id, name);
        set({ playback });
      });
    },

    async startReplay(channels) {
      const id = get().openId;
      if (!id) return;
      await attempt(async () => {
        const replay = await (await getBackend()).startReplay(id, get().playback?.positionMs ?? 0, channels);
        set({ replay });
      }, "replayError");
    },
    stopReplay: () =>
      attempt(async () => {
        set({ replay: await (await getBackend()).stopReplay() });
      }, "replayError"),
    restore: () =>
      attempt(async () => {
        set({ replay: await (await getBackend()).restoreBeforeReplay() });
      }, "replayError"),
    clearReplayMessage: () =>
      set((s) => ({ replay: s.replay ? { ...s.replay, message: null } : s.replay, replayError: null })),
  };
});
