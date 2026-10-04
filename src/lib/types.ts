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
