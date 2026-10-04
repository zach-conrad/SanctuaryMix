// Browser-demo stand-in for crates/recorder, playback and replay, so
// `npm run dev` shows a working Mix Manager. Everything lives in memory and
// the "audio" is a clock; nothing here ships in the app's control path.

import type { Backend } from "./backend";
import { buildRecording, seedRecordings, snapshotEvents, type DemoRecording } from "./demoRecordings";
import {
  DAY_MS,
  STEREO_BYTES_PER_HOUR,
  TRACK_BYTES_PER_HOUR,
  channelKey,
  defaultTitle,
  formatGb,
  requiredFreeBytes,
  stateAt,
} from "./recordings";
import type {
  ChangeSource,
  ChannelId,
  PlaybackStatus,
  RecordedEvent,
  RecorderStatus,
  RecordingSettings,
  RecordingSummary,
  ReplayStatus,
} from "./types";

export interface DemoDesk {
  channelCount(): number;
  name(ch: number): string;
  fader(ch: number): number | null;
  muted(ch: number): boolean;
  /** Sends a replayed change to the demo desk (and on to the UI). */
  setFader(ch: number, db: number | null): void;
  setMute(ch: number, muted: boolean): void;
  /** Replay switches auto-mix off first, like the real engine. */
  automixOff(): boolean;
}

type RecorderMethods = Pick<
  Backend,
  | "getRecordingSettings"
  | "setRecordingSettings"
  | "startRecording"
  | "stopRecording"
  | "recorderStatus"
  | "listRecordings"
  | "getRecording"
  | "recordingEvents"
  | "updateRecording"
  | "deleteRecording"
  | "diskUsage"
  | "deleteOldMultitracks"
  | "onRecorder"
  | "listOutputDevices"
  | "loadPlayback"
  | "play"
  | "pause"
  | "seek"
  | "unloadPlayback"
  | "onPlayback"
  | "startReplay"
  | "stopReplay"
  | "restoreBeforeReplay"
  | "onReplay"
>;

export interface DemoRecorder extends RecorderMethods {
  /** Call for every fader or mute change a person or Assist makes in the app. */
  noteChange(source: Extract<ChangeSource, "operator" | "assist">, id: ChannelId, change: { db: number | null } | { muted: boolean }): void;
}

/** A demo disk with room to spare: 500 GB, the rest used by other files. */
const DISK_FREE_BASE = 214e9;
const TICK_MS = 100;

function summary(r: DemoRecording): RecordingSummary {
  const { files: _files, channelNames: _names, ...rest } = r.detail;
  return { ...rest, eventCount: r.events.length };
}

export function createDemoRecorder(desk: DemoDesk): DemoRecorder {
  const recordings = new Map<string, DemoRecording>(seedRecordings().map((r) => [r.detail.id, r]));
  let settings: RecordingSettings = { mixChannels: [62, 63], multitrack: false };
  const recorderListeners = new Set<(s: RecorderStatus) => void>();
  const playbackListeners = new Set<(s: PlaybackStatus) => void>();
  const replayListeners = new Set<(s: ReplayStatus) => void>();

  // ---- Recorder ----
  let active: DemoRecording | null = null;
  let recTimer: ReturnType<typeof setInterval> | null = null;
  let warning: string | null = null;

  const usedBytes = () => [...recordings.values()].reduce((n, r) => n + r.detail.bytesOnDisk, 0);
  const freeBytes = () => DISK_FREE_BASE - usedBytes();
  const ratePerSec = (r: DemoRecording) =>
    ((r.detail.audioMode === "none" ? 0 : STEREO_BYTES_PER_HOUR) + r.detail.trackCount * TRACK_BYTES_PER_HOUR) / 3600;

  function recorderStatus(): RecorderStatus {
    const elapsedMs = active ? Date.now() - active.detail.startedAt : 0;
    return {
      active: active ? summary(active) : null,
      elapsedMs,
      bytesWritten: active ? active.detail.bytesOnDisk : 0,
      freeBytes: freeBytes(),
      warning,
    };
  }
  const pushRecorder = () => {
    const s = recorderStatus();
    recorderListeners.forEach((cb) => cb(s));
  };

  function tickRecording() {
    if (!active) return;
    const d = active.detail;
    d.durationMs = Date.now() - d.startedAt;
    d.bytesOnDisk = Math.round((ratePerSec(active) * d.durationMs) / 1000) + active.events.length * 120;
    d.eventCount = active.events.length;
    pushRecorder();
  }

  function appendEvent(rec: DemoRecording, source: ChangeSource, event: RecordedEvent["event"]) {
    const tMs = Date.now() - rec.detail.startedAt;
    const last = rec.events[rec.events.length - 1];
    // Fader drags arrive many times a second; keep the last value of each burst.
    if (
      last &&
      last.source === source &&
      event.type === "fader" &&
      last.event.type === "fader" &&
      channelKey(last.event.id) === channelKey(event.id) &&
      tMs - last.tMs < 250
    ) {
      last.event = event;
      return;
    }
    rec.events.push({ seq: rec.events.length, tMs, source, event });
  }

  // ---- Playback (a clock; the demo has no audio) ----
  let playback: PlaybackStatus | null = null;
  let playTimer: ReturnType<typeof setInterval> | null = null;
  let lastTick = 0;

  const pushPlayback = () => {
    if (!playback) return;
    const s = { ...playback };
    playbackListeners.forEach((cb) => cb(s));
  };

  function advance() {
    if (!playback) return;
    const now = performance.now();
    if (playback.playing) {
      const from = playback.positionMs;
      const to = Math.min(playback.durationMs, from + (now - lastTick));
      playback.positionMs = to;
      sendReplayed(from, to);
      if (to >= playback.durationMs) {
        playback.playing = false;
        if (replay.state === "running") {
          replay = { ...replay, state: "stopped", message: "Reached the end of the service. The console keeps the last moves." };
          pushReplay();
        }
      }
    }
    lastTick = now;
    pushPlayback();
  }

  // ---- Replay ----
  let replay: ReplayStatus = {
    state: "idle",
    recordingId: null,
    channels: null,
    sentCount: 0,
    canRestore: false,
    message: null,
  };
  let before: { ch: number; db: number | null; muted: boolean }[] | null = null;

  const pushReplay = () => {
    const s = { ...replay };
    replayListeners.forEach((cb) => cb(s));
  };
  const wanted = (id: ChannelId) =>
    id.kind === "input" &&
    id.index < desk.channelCount() &&
    (replay.channels === null || replay.channels.some((c) => channelKey(c) === channelKey(id)));

  function send(event: RecordedEvent["event"]) {
    if (event.type === "fader" && wanted(event.id)) desk.setFader(event.id.index, event.db);
    else if (event.type === "mute" && wanted(event.id)) desk.setMute(event.id.index, event.muted);
    else return;
    replay.sentCount++;
  }

  /** Puts the console where the recording was at `tMs` (chosen channels only). */
  function sendStateAt(tMs: number) {
    const rec = replay.recordingId ? recordings.get(replay.recordingId) : null;
    if (!rec) return;
    for (const s of stateAt(rec.events, tMs)) {
      send({ type: "fader", id: s.id, db: s.faderDb });
      send({ type: "mute", id: s.id, muted: s.muted });
    }
    pushReplay();
  }

  function sendReplayed(fromMs: number, toMs: number) {
    if (replay.state !== "running" || replay.recordingId !== playback?.recordingId) return;
    const rec = recordings.get(replay.recordingId!);
    if (!rec) return;
    let sent = false;
    for (const e of rec.events) {
      if (e.source === "snapshot" || e.tMs <= fromMs || e.tMs > toMs) continue;
      send(e.event);
      sent = true;
    }
    if (sent) pushReplay();
  }

  function get(id: string): DemoRecording {
    const r = recordings.get(id);
    if (!r) throw new Error("That recording isn't on this computer any more.");
    return r;
  }

  const api: DemoRecorder = {
    noteChange(source, id, change) {
      const event: RecordedEvent["event"] =
        "db" in change ? { type: "fader", id, db: change.db } : { type: "mute", id, muted: change.muted };
      if (active) appendEvent(active, source, event);
      if (source === "operator" && replay.state === "running") {
        const name = id.kind === "input" ? desk.name(id.index) : channelKey(id);
        replay = {
          ...replay,
          state: "takenOver",
          message: `Stopped because ${name} was changed in SanctuaryMix. You have the console.`,
        };
        pushReplay();
      }
    },

    async getRecordingSettings() {
      return settings;
    },
    async setRecordingSettings(next) {
      settings = next;
    },
    async startRecording(title) {
      if (active) throw new Error("A service is already recording.");
      const stereo = settings.mixChannels !== null;
      const tracks = settings.multitrack ? desk.channelCount() : 0;
      const need = requiredFreeBytes(stereo, tracks);
      if (need > freeBytes()) {
        throw new Error(
          `There isn't enough free disk space to start (needs ${formatGb(need)}). Delete old multitracks in Services or turn multitrack off.`,
        );
      }
      const startedAt = Date.now();
      const count = desk.channelCount();
      const names = Array.from({ length: count }, (_, i) => desk.name(i));
      const events = snapshotEvents(
        names,
        names.map((_, i) => desk.fader(i)),
        (i) => desk.muted(i),
      );
      active = buildRecording(`rec-${startedAt.toString(36)}`, startedAt, 0, {
        title: title?.trim() || defaultTitle(startedAt),
        audioMode: !stereo ? "none" : tracks > 0 ? "stereoMultitrack" : "stereo",
        status: "recording",
        events,
        trackCount: tracks,
      });
      recordings.set(active.detail.id, active);
      warning = null;
      recTimer = setInterval(tickRecording, 1000);
      pushRecorder();
      return summary(active);
    },
    async stopRecording() {
      if (!active) return null;
      tickRecording();
      if (recTimer) clearInterval(recTimer);
      recTimer = null;
      const done = active;
      done.detail.status = "complete";
      done.detail.endedAt = Date.now();
      // Refresh file sizes now the length is final.
      const final = buildRecording(done.detail.id, done.detail.startedAt, done.detail.durationMs, {
        title: done.detail.title,
        audioMode: done.detail.audioMode,
        status: "complete",
        events: done.events,
        notes: done.detail.notes,
        trackCount: done.detail.trackCount,
      });
      recordings.set(done.detail.id, final);
      active = null;
      warning = null;
      pushRecorder();
      return summary(final);
    },
    async recorderStatus() {
      return recorderStatus();
    },
    async listRecordings() {
      return [...recordings.values()].map(summary).sort((a, b) => b.startedAt - a.startedAt);
    },
    async getRecording(id) {
      const r = get(id);
      return { ...r.detail, eventCount: r.events.length };
    },
    async recordingEvents(id) {
      return [...get(id).events];
    },
    async updateRecording(id, title, notes) {
      const r = get(id);
      r.detail.title = title.trim() || r.detail.title;
      r.detail.notes = notes;
      return summary(r);
    },
    async deleteRecording(id) {
      if (active?.detail.id === id) throw new Error("Stop the recording before deleting it.");
      if (playback?.recordingId === id) await api.unloadPlayback();
      recordings.delete(id);
    },
    async diskUsage() {
      const multitrackBytes = [...recordings.values()]
        .flatMap((r) => r.detail.files)
        .filter((f) => f.kind === "track")
        .reduce((n, f) => n + f.bytes, 0);
      return {
        freeBytes: freeBytes(),
        recordingsBytes: usedBytes(),
        multitrackBytes,
        folder: "~/Library/Application Support/SanctuaryMix/recordings",
      };
    },
    async deleteOldMultitracks(days) {
      const cutoff = Date.now() - days * DAY_MS;
      let freed = 0;
      for (const r of recordings.values()) {
        if (r === active || r.detail.startedAt >= cutoff) continue;
        const tracks = r.detail.files.filter((f) => f.kind === "track");
        if (tracks.length === 0) continue;
        const bytes = tracks.reduce((n, f) => n + f.bytes, 0);
        freed += bytes;
        r.detail.files = r.detail.files.filter((f) => f.kind !== "track");
        r.detail.bytesOnDisk -= bytes;
        r.detail.trackCount = 0;
        if (r.detail.audioMode === "stereoMultitrack") r.detail.audioMode = "stereo";
      }
      return freed;
    },
    async onRecorder(cb) {
      recorderListeners.add(cb);
      return () => recorderListeners.delete(cb);
    },

    async listOutputDevices() {
      return [
        { name: "MacBook Pro Speakers", maxOutputChannels: 2, isDefault: true, isDante: false },
        { name: "Dante Virtual Soundcard", maxOutputChannels: 64, isDefault: false, isDante: true },
        { name: "External Headphones", maxOutputChannels: 2, isDefault: false, isDante: false },
      ];
    },
    async loadPlayback(id, device) {
      const r = get(id);
      const keep = playback?.recordingId === id ? playback.positionMs : 0;
      playback = {
        recordingId: id,
        positionMs: keep,
        durationMs: r.detail.durationMs,
        playing: playback?.recordingId === id ? playback.playing : false,
        device: device ?? "MacBook Pro Speakers",
        hasAudio: r.detail.audioMode !== "none",
      };
      lastTick = performance.now();
      playTimer ??= setInterval(advance, TICK_MS);
      pushPlayback();
      return { ...playback };
    },
    async play() {
      if (!playback) return;
      if (playback.positionMs >= playback.durationMs) playback.positionMs = 0;
      playback.playing = true;
      lastTick = performance.now();
      pushPlayback();
    },
    async pause() {
      if (!playback) return;
      playback.playing = false;
      pushPlayback();
    },
    async seek(positionMs) {
      if (!playback) return;
      playback.positionMs = Math.max(0, Math.min(playback.durationMs, positionMs));
      if (replay.state === "running") sendStateAt(playback.positionMs);
      pushPlayback();
    },
    async unloadPlayback() {
      if (replay.state === "running") {
        replay = { ...replay, state: "stopped", message: "Stopped because the service was closed." };
        pushReplay();
      }
      if (playTimer) clearInterval(playTimer);
      playTimer = null;
      playback = null;
    },
    async onPlayback(cb) {
      playbackListeners.add(cb);
      return () => playbackListeners.delete(cb);
    },

    async startReplay(id, fromMs, channels) {
      if (active) throw new Error("Stop recording first. Moves can't be sent while a service is being recorded.");
      get(id);
      const automixWasOn = desk.automixOff();
      before = Array.from({ length: desk.channelCount() }, (_, ch) => ({
        ch,
        db: desk.fader(ch),
        muted: desk.muted(ch),
      }));
      replay = {
        state: "running",
        recordingId: id,
        channels,
        sentCount: 0,
        canRestore: true,
        message: automixWasOn ? "Auto-mix was switched off for the replay." : null,
      };
      if (playback?.recordingId !== id) await api.loadPlayback(id, playback?.device ?? null);
      playback!.positionMs = fromMs;
      sendStateAt(fromMs);
      await api.play();
      return { ...replay };
    },
    async stopReplay() {
      if (replay.state === "running") {
        replay = { ...replay, state: "stopped", message: "Stopped. The console keeps the faders and mutes it has now." };
        if (playback) playback.playing = false;
        pushPlayback();
        pushReplay();
      }
      return { ...replay };
    },
    async restoreBeforeReplay() {
      if (!before) throw new Error("There's nothing to restore.");
      if (replay.state === "running") await api.stopReplay();
      for (const s of before) {
        desk.setFader(s.ch, s.db);
        desk.setMute(s.ch, s.muted);
      }
      before = null;
      replay = {
        state: "idle",
        recordingId: replay.recordingId,
        channels: null,
        sentCount: 0,
        canRestore: false,
        message: "Console restored to how it was before the replay.",
      };
      pushReplay();
      return { ...replay };
    },
    async onReplay(cb) {
      replayListeners.add(cb);
      return () => replayListeners.delete(cb);
    },
  };
  return api;
}
