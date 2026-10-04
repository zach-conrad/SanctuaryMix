// The one place the UI talks to the Rust core. Inside the app this calls Tauri
// commands; in a plain browser (`npm run dev`) it falls back to a demo backend
// so the UI can be designed and reviewed without a Mac build or a console.

import type {
  AudioDeviceInfo,
  ChannelId,
  ConsoleConfig,
  ConsoleEvent,
  MeterFrame,
  MeteringInfo,
  MicAccess,
  Session,
  DiskUsage,
  OutputDeviceInfo,
  PlaybackStatus,
  RecordedEvent,
  RecorderStatus,
  RecordingDetail,
  RecordingSettings,
  RecordingSummary,
  ReplayStatus,
} from "./types";
import { createDemoBackend } from "./demoBackend";

type Unlisten = () => void;

export interface Backend {
  readonly isDemo: boolean;
  microphoneAccess(): Promise<MicAccess>;
  /** Shows the macOS prompt if the user hasn't answered yet. */
  requestMicrophoneAccess(): Promise<MicAccess>;
  openMicrophoneSettings(): Promise<void>;
  listAudioDevices(): Promise<AudioDeviceInfo[]>;
  startMetering(device: string | null): Promise<MeteringInfo>;
  stopMetering(): Promise<void>;
  connectConsole(config: ConsoleConfig): Promise<void>;
  disconnectConsole(): Promise<void>;
  setFader(id: ChannelId, db: number | null): Promise<void>;
  setMute(id: ChannelId, muted: boolean): Promise<void>;
  getSession(): Promise<Session>;
  onMeters(cb: (frame: MeterFrame) => void): Promise<Unlisten>;
  onConsole(cb: (event: ConsoleEvent) => void): Promise<Unlisten>;

  // Service recordings (docs/RECORDINGS.md)
  getRecordingSettings(): Promise<RecordingSettings>;
  setRecordingSettings(settings: RecordingSettings): Promise<void>;
  /** Starts recording the current service. Audio follows the saved settings. */
  startRecording(title: string | null): Promise<RecordingSummary>;
  stopRecording(): Promise<RecordingSummary | null>;
  recorderStatus(): Promise<RecorderStatus>;
  listRecordings(): Promise<RecordingSummary[]>;
  getRecording(id: string): Promise<RecordingDetail>;
  recordingEvents(id: string): Promise<RecordedEvent[]>;
  updateRecording(id: string, title: string, notes: string): Promise<RecordingSummary>;
  deleteRecording(id: string): Promise<void>;
  diskUsage(): Promise<DiskUsage>;
  /** Deletes multitrack files older than `days`; returns bytes freed. Mix and moves are kept. */
  deleteOldMultitracks(days: number): Promise<number>;
  onRecorder(cb: (status: RecorderStatus) => void): Promise<Unlisten>;

  // Playback
  listOutputDevices(): Promise<OutputDeviceInfo[]>;
  loadPlayback(id: string, device: string | null): Promise<PlaybackStatus>;
  play(): Promise<void>;
  pause(): Promise<void>;
  seek(positionMs: number): Promise<void>;
  unloadPlayback(): Promise<void>;
  onPlayback(cb: (status: PlaybackStatus) => void): Promise<Unlisten>;

  // Replay to console (guarded; Engineer and Admin only)
  startReplay(id: string, fromMs: number, channels: ChannelId[] | null): Promise<ReplayStatus>;
  stopReplay(): Promise<ReplayStatus>;
  restoreBeforeReplay(): Promise<ReplayStatus>;
  onReplay(cb: (status: ReplayStatus) => void): Promise<Unlisten>;
}

function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function createTauriBackend(): Promise<Backend> {
  const { invoke } = await import("@tauri-apps/api/core");
  const { listen } = await import("@tauri-apps/api/event");
  return {
    isDemo: false,
    microphoneAccess: () => invoke("microphone_access"),
    requestMicrophoneAccess: () => invoke("request_microphone_access"),
    openMicrophoneSettings: () => invoke("open_microphone_settings"),
    listAudioDevices: () => invoke("list_audio_devices"),
    startMetering: (device) => invoke("start_metering", { device }),
    stopMetering: () => invoke("stop_metering"),
    connectConsole: (config) => invoke("connect_console", { config }),
    disconnectConsole: () => invoke("disconnect_console"),
    setFader: (id, db) => invoke("set_fader", { id, db }),
    setMute: (id, muted) => invoke("set_mute", { id, muted }),
    getSession: () => invoke("get_session"),
    onMeters: (cb) => listen<MeterFrame>("meters", (e) => cb(e.payload)),
    onConsole: (cb) => listen<ConsoleEvent>("console", (e) => cb(e.payload)),

    getRecordingSettings: () => invoke("get_recording_settings"),
    setRecordingSettings: (settings) => invoke("set_recording_settings", { settings }),
    startRecording: (title) => invoke("start_recording", { title }),
    stopRecording: () => invoke("stop_recording"),
    recorderStatus: () => invoke("recorder_status"),
    listRecordings: () => invoke("list_recordings"),
    getRecording: (id) => invoke("get_recording", { id }),
    recordingEvents: (id) => invoke("recording_events", { id }),
    updateRecording: (id, title, notes) => invoke("update_recording", { id, title, notes }),
    deleteRecording: (id) => invoke("delete_recording", { id }),
    diskUsage: () => invoke("disk_usage"),
    deleteOldMultitracks: (days) => invoke("delete_old_multitracks", { days }),
    onRecorder: (cb) => listen<RecorderStatus>("recording", (e) => cb(e.payload)),

    listOutputDevices: () => invoke("list_output_devices"),
    loadPlayback: (id, device) => invoke("load_playback", { id, device }),
    play: () => invoke("play_recording"),
    pause: () => invoke("pause_recording"),
    seek: (positionMs) => invoke("seek_recording", { positionMs }),
    unloadPlayback: () => invoke("unload_playback"),
    onPlayback: (cb) => listen<PlaybackStatus>("playback", (e) => cb(e.payload)),

    startReplay: (id, fromMs, channels) => invoke("start_replay", { id, fromMs, channels }),
    stopReplay: () => invoke("stop_replay"),
    restoreBeforeReplay: () => invoke("restore_before_replay"),
    onReplay: (cb) => listen<ReplayStatus>("replay", (e) => cb(e.payload)),
  };
}

let backend: Promise<Backend> | null = null;

export function getBackend(): Promise<Backend> {
  backend ??= isTauri() ? createTauriBackend() : Promise.resolve(createDemoBackend());
  return backend;
}
