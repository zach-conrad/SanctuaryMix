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
  | { type: "name"; id: ChannelId; name: string }
  | { type: "eq"; id: ChannelId; change: EqChange };

// ---- Channel EQ (crates/mix-core/src/eq.rs) ----

/** Band shape. Bands 1 and 2 (0-based) are always bells; band 0 can also be a
 * low shelf or high-pass, band 3 a high shelf or low-pass. */
export type EqBandKind = "bell" | "lowShelf" | "highShelf" | "lowPass" | "highPass";

export interface EqBand {
  kind: EqBandKind;
  freqHz: number;
  /** Bandwidth in octaves, as the desk labels it (1.5 wide to 1/9 narrow). */
  width: number;
  gainDb: number;
}

export interface Hpf {
  on: boolean;
  freqHz: number;
}

/** Bands are 0-3 here and shown as 1-4. Band 3 (shown as 4) is kept for feedback notches. */
export interface ChannelEq {
  hpf: Hpf;
  bands: [EqBand, EqBand, EqBand, EqBand];
}

export type EqChange =
  | { param: "bandKind"; band: number; kind: EqBandKind }
  | { param: "bandFreq"; band: number; hz: number }
  | { param: "bandWidth"; band: number; width: number }
  | { param: "bandGain"; band: number; db: number }
  | { param: "hpfOn"; on: boolean }
  | { param: "hpfFreq"; hz: number };

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

// Plans (crates/auth/src/plan.rs).
export type Plan = "essentials" | "pro" | "campus";
export type SubscriptionStatus =
  | "trialing"
  | "active"
  | "pastDue"
  | "paused"
  | "canceled"
  | "unpaid"
  | "incompleteExpired";
export type Feature = "autoMix" | "recordServices" | "cloudSync" | "mixReports" | "aiEq";

export interface Entitlements {
  maxAiChannels: number;
  autoMix: boolean;
  recordServices: boolean;
  cloudSync: boolean;
  mixReports: boolean;
  aiEq: boolean;
  /** null is unlimited. */
  teamLogins: number | null;
  rooms: number;
}

export interface Access {
  /** null when signed out. */
  plan: Plan | null;
  status: SubscriptionStatus | null;
  entitlements: Entitlements;
}

/** Things only some roles may do (crates/auth `Permission`). */
export type Permission = "changeAutoMixSetup" | "chooseAdminFeel" | "deleteRecordings" | "replayToConsole";

export interface Session {
  user: { id: string; displayName: string; email: string | null };
  activeOrg: { id: string; name: string } | null;
  role: Role;
  authenticated: boolean;
  access: Access;
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

// ---- AI EQ (crates/tonal/src/types.rs, docs/AIEQ.md) ----

export type TapPoint = "beforeEq" | "afterEq";

export interface AiEqConfig {
  /** AI EQ's own on/off, separate from auto-mix. Channels and roles are auto-mix's. */
  enabled: boolean;
  /** Small, slow tone moves on speech mics during the service (Engineers and Admins). */
  toneKeeping: boolean;
  tap: TapPoint;
}

export type EqMode =
  | "off"
  | "consoleOffline"
  | "notSupported"
  | "reading"
  | "frozen"
  | "notChecked"
  | "set"
  | "keeping"
  | "notch"
  | "yours"
  | "undone";

export interface EqProposal {
  title: string;
  reason: string;
  /** The whole EQ as it would be on the desk. */
  eq: ChannelEq;
  /** Each change as shown, e.g. "320 Hz  0.0 → −2.0 dB". */
  changes: string[];
}

export interface Notch {
  hz: number;
  gainDb: number;
  atMs: number;
}

export interface EqChannelStatus {
  channel: number;
  name: string | null;
  role: ChannelRole;
  mode: EqMode;
  eq: ChannelEq | null;
  baseline: ChannelEq | null;
  proposal: EqProposal | null;
  notch: Notch | null;
  toneOffsetDb: number | null;
  /** What the mic hears: 60 values, 1/6 octave from 20 Hz (see SPECTRUM_FREQS), dB relative to its average. */
  spectrum: number[] | null;
  heardSecs: number;
  differsFromProfile: boolean;
}

export type SoundcheckState =
  | "upNext"
  | "listening"
  | "done"
  | "soundsGood"
  | "noSound"
  | "applied"
  | "skipped";

export interface SoundcheckChannel {
  channel: number;
  state: SoundcheckState;
  heardSecs: number;
  changes: number;
}

export interface SoundcheckStatus {
  running: boolean;
  channels: SoundcheckChannel[];
}

export type CompareSide = "before" | "after";

export interface FeedbackEvent {
  id: number;
  atMs: number;
  channel: number;
  channelName: string;
  hz: number;
  /** Null when band 4 was already holding a notch (fader only). */
  notchDb: number | null;
  faderCutDb: number;
  countToday: number;
}

export interface RingResult {
  channel: number;
  hz: number | null;
  ceilingDb: number | null;
}

export interface RingOutStatus {
  running: boolean;
  channel: number | null;
  results: RingResult[];
}

export interface AiEqStatus {
  enabled: boolean;
  frozen: boolean;
  consoleOnline: boolean;
  eqSupported: boolean;
  audioOk: boolean;
  toneKeeping: boolean;
  /** Recording or auto-mix on: soundcheck changes and Before | After wait. */
  inService: boolean;
  soundcheck: SoundcheckStatus | null;
  compare: { channel: number; side: CompareSide } | null;
  ringOut: RingOutStatus | null;
  feedback: FeedbackEvent | null;
  profileDiffers: number[];
  channels: EqChannelStatus[];
}

/** Matches eq_audit.action in docs/supabase/eq_audit.sql. */
export type EqAction = "soundcheck" | "feedback" | "tone" | "undo" | "person" | "handBack";

export interface EqActor {
  kind: "ai" | "person";
  role: Role | null;
  name: string | null;
  userId: string | null;
  where: "app" | "desk" | null;
}

export interface EqLogEntry {
  atMs: number;
  channel: number;
  channelName: string;
  action: EqAction;
  changes: string[];
  reason: string | null;
  by: EqActor;
  appliedBy: EqActor | null;
}

export type IdeaState = "waiting" | "kept" | "dismissed";

export interface EqIdea {
  id: string;
  recordingId: string | null;
  atMs: number;
  channel: number;
  channelName: string;
  title: string;
  change: string;
  reason: string;
  state: IdeaState;
}

/** A service's EQ changes and ideas, as the website shows them. */
export interface EqAudit {
  entries: (EqLogEntry & { tMs: number | null })[];
  ideas: EqIdea[];
}
