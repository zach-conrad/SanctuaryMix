// Mirrors of the Rust types in crates/mix-core, crates/console and crates/auth.
// Keep these in sync when the Rust side changes.

export type ChannelKind = "input" | "group" | "aux" | "matrix" | "main" | "dca" | "fxReturn";

export interface ChannelId {
  kind: ChannelKind;
  index: number;
}

export interface ChannelMeter {
  channel: number;
  peakDb: number;
  rmsDb: number;
  clipped: boolean;
}

export interface MeterFrame {
  sampleRate: number;
  channels: ChannelMeter[];
}

export type ConsoleEvent =
  | { type: "connected"; model: string }
  | { type: "disconnected"; reason: string | null }
  | { type: "fader"; id: ChannelId; db: number | null }
  | { type: "mute"; id: ChannelId; muted: boolean }
  | { type: "name"; id: ChannelId; name: string };

export type ConsoleModel = "dlive" | "simulated";

export interface ConsoleConfig {
  model: ConsoleModel;
  host: string;
  port: number | null;
  midiChannel: number;
  inputCount: number;
}

/** macOS microphone permission. Always "granted" off macOS. */
export type MicAccess = "granted" | "undetermined" | "denied";

export interface AudioDeviceInfo {
  name: string;
  maxInputChannels: number;
  defaultSampleRate: number;
  isDefault: boolean;
  isDante: boolean;
}

export interface MeteringInfo {
  deviceName: string;
  channels: number;
  sampleRate: number;
}

export type Role = "admin" | "engineer" | "volunteer";

export interface Session {
  user: { id: string; displayName: string; email: string | null };
  activeOrg: { id: string; name: string } | null;
  role: Role;
  authenticated: boolean;
}

// ---- Service recordings (crates/recorder, docs/RECORDINGS.md) ----

/** Who made a control change. Mirrors mix_core::ChangeSource. */
export type ChangeSource = "console" | "operator" | "assist" | "replay" | "snapshot";

export type AudioMode = "none" | "stereo" | "stereoMultitrack";
export type RecordingStatusKind = "recording" | "complete" | "interrupted";
export type SyncState = "localOnly" | "pending" | "uploading" | "synced" | "failed";

/** One row in the Mix Manager list. Times are epoch ms (UTC). */
export interface RecordingSummary {
  id: string;
  orgId: string | null;
  createdBy: string;
  title: string;
  /** Local calendar date YYYY-MM-DD; services are grouped by this. */
  serviceDate: string;
  startedAt: number;
  endedAt: number | null;
  durationMs: number;
  status: RecordingStatusKind;
  consoleModel: string;
  sampleRate: number | null;
  audioMode: AudioMode;
  mixChannels: [number, number] | null;
  trackCount: number;
  bytesOnDisk: number;
  notes: string;
  eventCount: number;
  syncState: SyncState;
}

export interface RecordingFile {
  kind: "mix" | "track" | "events" | "manifest";
  /** Device input (0-based) for tracks; null for the mix. */
  channel: number | null;
  relPath: string;
  bytes: number;
}

export interface RecordingDetail extends RecordingSummary {
  files: RecordingFile[];
  /** Channel names as they were when the recording started. */
  channelNames: { id: ChannelId; name: string }[];
}

/** A control change inside a recording; tMs is the offset from its start. */
export interface RecordedEvent {
  seq: number;
  tMs: number;
  source: ChangeSource;
  event: ConsoleEvent;
}

/** Live state of the recorder, pushed as the "recording" event about once a second. */
export interface RecorderStatus {
  active: RecordingSummary | null;
  elapsedMs: number;
  bytesWritten: number;
  freeBytes: number;
  /** Plain-language warning, e.g. multitrack stopped because the disk is nearly full. */
  warning: string | null;
}

export interface RecordingSettings {
  /** Dante inputs (0-based) carrying Main L/R, or null if not chosen yet. */
  mixChannels: [number, number] | null;
  multitrack: boolean;
}

export interface DiskUsage {
  freeBytes: number;
  recordingsBytes: number;
  multitrackBytes: number;
  folder: string;
}

export interface OutputDeviceInfo {
  name: string;
  maxOutputChannels: number;
  isDefault: boolean;
  isDante: boolean;
}

/** Pushed as the "playback" event about 10x a second while loaded. */
export interface PlaybackStatus {
  recordingId: string | null;
  positionMs: number;
  durationMs: number;
  playing: boolean;
  device: string | null;
  /** False when the recording has no audio; the timeline still plays. */
  hasAudio: boolean;
}

/** Sending recorded moves back to a console. Pushed as the "replay" event. */
export interface ReplayStatus {
  state: "idle" | "running" | "stopped" | "takenOver";
  recordingId: string | null;
  /** Only these channels are sent; null means all. */
  channels: ChannelId[] | null;
  sentCount: number;
  /** True when a pre-replay snapshot exists and Restore is available. */
  canRestore: boolean;
  message: string | null;
}
