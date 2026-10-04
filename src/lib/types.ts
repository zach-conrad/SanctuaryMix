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

// Auto-mix (crates/automix).

export type ChannelRole =
  | "speech"
  | "leadVocal"
  | "backingVocal"
  | "choir"
  | "kick"
  | "bass"
  | "drums"
  | "keysPads"
  | "pianoOrgan"
  | "electricGuitar"
  | "acousticGuitar"
  | "playback"
  | "other";

export type RoomFeel = "fullModern" | "bigLoud" | "deepLowEnd" | "warmIntimate" | "traditionalChoral" | "spokenWord";

export interface Preset {
  feel: RoomFeel;
  name: string;
  description: string;
  roomLevel: string;
  referenceDb: number;
  speechReferenceDb: number;
  /** dB under the lead vocal: backing vocal, choir, kick, bass, drums, keys, piano, electric gtr, acoustic gtr, playback, other. */
  balance: number[];
  speechOverMusicDb: number;
  adminOnly: boolean;
  warning: string | null;
}

export interface Nudges {
  loudnessDb: number;
  lowEndDb: number;
  vocalPresenceDb: number;
}

export interface RideLimits {
  maxStepDb: number;
  maxRateDbPerSec: number;
  deadbandDb: number;
  maxBoostDb: number;
  maxCutDb: number;
}

export interface Guardrails {
  music: RideLimits;
  speech: RideLimits;
  gateDbfs: number;
  noRaiseAbovePeakDbfs: number;
}

export interface ManagedChannel {
  channel: number;
  role: ChannelRole;
}

export interface AutoMixConfig {
  feel: RoomFeel;
  nudges: Nudges;
  channels: ManagedChannel[];
  guardrails: Guardrails;
  /** Use the on-device listening models to tell voices from bleed. */
  listen: boolean;
}

/** What the on-device listening models hear on a mic (mix-core/src/hearing.rs). */
export type Sound =
  | "speech"
  | "singing"
  | "choir"
  | "drums"
  | "bass"
  | "electricGuitar"
  | "acousticGuitar"
  | "piano"
  | "organ"
  | "keys"
  | "brass"
  | "strings"
  | "music"
  | "other";

/** What an input has mostly sounded like during a listen. */
export interface HeardChannel {
  channel: number;
  sound: Sound;
  /** Share of what was heard that was `sound`, 0 to 1. */
  share: number;
  seconds: number;
  /** Set when what it hears doesn't fit the role it has. */
  suggestedRole: ChannelRole | null;
}

export type ChannelMode =
  | "off"
  | "consoleOffline"
  | "frozen"
  | "heldByOperator"
  | "undone"
  | "waitingForFader"
  | "muted"
  | "clipping"
  | "bleed"
  | "noAudio"
  | "idle"
  | "waitingForLead"
  | "riding"
  | "atLimit"
  | "settled";

export interface ChannelStatus {
  channel: number;
  name: string | null;
  role: ChannelRole;
  mode: ChannelMode;
  faderDb: number | null;
  baselineDb: number | null;
  targetDb: number | null;
  levelDb: number | null;
  /** What the listening models hear on it, if they're running. */
  heard: Sound | null;
  /** Someone is talking or singing into it, if listening. */
  voice: boolean | null;
}

export interface AutoMixStatus {
  engaged: boolean;
  frozen: boolean;
  consoleOnline: boolean;
  audioOk: boolean;
  /** The listening models are running and reporting. */
  listening: boolean;
  feel: RoomFeel;
  speechChannel: number | null;
  leadChannel: number | null;
  channels: ChannelStatus[];
}

export type AdjustmentKind =
  | "auto"
  | "undo"
  | "operatorTookOver"
  | "engaged"
  | "disengaged"
  | "frozen"
  | "resumed"
  | "channelResumed";

export interface Adjustment {
  atMs: number;
  kind: AdjustmentKind;
  channel: number | null;
  channelName: string | null;
  fromDb: number | null;
  toDb: number | null;
  reason: string;
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

// Room loudness (crates/mix-core/src/spl.rs)

export type Weighting = "a" | "c";

export interface SplConfig {
  /** 0-based input on the audio device; null turns the meter off. */
  source: number | null;
  /** dB SPL = dBFS (RMS) + this. */
  offsetDb: number;
  /** True while the offset came from a calibration that still matches the source, device and sample rate. */
  calibrated: boolean;
  /** What the last calibration was made against. */
  calibration: CalibrationRecord | null;
}

export interface CalibrationRecord {
  source: number;
  device: string;
  sampleRate: number;
  weighting: Weighting;
  referenceDb: number;
  /** Unix time in ms. */
  atMs: number;
}

/** Progress of a guided calibration (`spl-calibration` events). */
export type CalibrationStatus =
  | {
      state: "listening";
      levelDbfs: number;
      steadySecs: number;
      neededSecs: number;
      elapsedSecs: number;
      /** Why the steady count isn't growing, if it isn't. */
      hold: string | null;
    }
  | { state: "done"; offsetDb: number; config: SplConfig }
  | { state: "failed"; reason: string };

/** One A/C pair of levels in dB SPL. */
export interface AcLevel {
  a: number;
  c: number;
}

export interface SplReading {
  source: number;
  calibrated: boolean;
  /** 125 ms time weighting. */
  fast: AcLevel;
  /** 1 s time weighting. */
  slow: AcLevel;
  leq1m: AcLevel;
  leq15m: AcLevel;
  /** Leq since the last reset. */
  leqTotal: AcLevel;
  /** LAFmax since the reset. */
  aMax: number;
  /** LCpeak since the reset. */
  cPeak: number;
  seconds: number;
}

/** One second of history: that second's Leq. `t` is seconds since the reset. */
export interface SplPoint {
  t: number;
  a: number;
  c: number;
}
