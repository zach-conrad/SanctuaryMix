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
  };
}

let backend: Promise<Backend> | null = null;

export function getBackend(): Promise<Backend> {
  backend ??= isTauri() ? createTauriBackend() : Promise.resolve(createDemoBackend());
  return backend;
}
