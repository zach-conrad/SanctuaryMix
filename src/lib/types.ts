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
}

export interface AutoMixStatus {
  engaged: boolean;
  frozen: boolean;
  consoleOnline: boolean;
  audioOk: boolean;
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
