// Pure helpers for service recordings (docs/RECORDINGS.md): state at a point in
// time, the event list with before/after values, timeline lanes, and the
// formatting and disk math the Mix Manager shows.

import { formatDb } from "./levels";
import type { AudioMode, ChangeSource, ChannelId, RecordedEvent, RecordingSummary, SyncState } from "./types";

export interface ChannelState {
  id: ChannelId;
  name: string;
  faderDb: number | null;
  muted: boolean;
}

export const channelKey = (id: ChannelId): string => `${id.kind}:${id.index}`;

const KIND_ORDER: ChannelId["kind"][] = ["input", "fxReturn", "group", "aux", "matrix", "dca", "main"];

/** The console's own short names: "Ch 12", "Mix 3", "DCA 2". */
export function channelLabel(id: ChannelId): string {
  const n = id.index + 1;
  switch (id.kind) {
    case "input":
      return `Ch ${n}`;
    case "aux":
      return `Mix ${n}`;
    case "group":
      return `Group ${n}`;
    case "matrix":
      return `Matrix ${n}`;
    case "dca":
      return `DCA ${n}`;
    case "fxReturn":
      return `FX return ${n}`;
    case "main":
      return "Main";
  }
}

function compareIds(a: ChannelId, b: ChannelId): number {
  return KIND_ORDER.indexOf(a.kind) - KIND_ORDER.indexOf(b.kind) || a.index - b.index;
}

function blank(id: ChannelId): ChannelState {
  return { id, name: channelLabel(id), faderDb: 0, muted: false };
}

function apply(state: Map<string, ChannelState>, e: RecordedEvent): ChannelState | null {
  const ev = e.event;
  if (ev.type !== "fader" && ev.type !== "mute" && ev.type !== "name") return null;
  const key = channelKey(ev.id);
  const cur = state.get(key) ?? blank(ev.id);
  const next =
    ev.type === "fader"
      ? { ...cur, faderDb: ev.db }
      : ev.type === "mute"
        ? { ...cur, muted: ev.muted }
        : { ...cur, name: ev.name || channelLabel(ev.id) };
  state.set(key, next);
  return next;
}

/**
 * Console state at `tMs`: the opening snapshot plus every change up to and
 * including that moment. Events must be in recording order (by seq).
 */
export function stateAt(events: RecordedEvent[], tMs: number): ChannelState[] {
  const state = new Map<string, ChannelState>();
  for (const e of events) {
    if (e.tMs > tMs) break;
    apply(state, e);
  }
  return [...state.values()].sort((a, b) => compareIds(a.id, b.id));
}

export interface EventRow {
  seq: number;
  tMs: number;
  source: ChangeSource;
  id: ChannelId | null;
  name: string;
  /** "−4.5 dB → −2.0 dB", "Muted", "Unmuted", "Renamed Lapel 1 → Pastor". */
  change: string;
  kind: RecordedEvent["event"]["type"];
}

const fmtFader = (db: number | null) => (db === null || db <= -90 ? formatDb(db) : `${formatDb(db)} dB`);

/** Every change after the opening snapshot, with the value it replaced. */
export function describeEvents(events: RecordedEvent[]): EventRow[] {
  const state = new Map<string, ChannelState>();
  const rows: EventRow[] = [];
  for (const e of events) {
    const ev = e.event;
    if (ev.type === "connected" || ev.type === "disconnected") {
      if (e.source !== "snapshot") {
        rows.push({
          seq: e.seq,
          tMs: e.tMs,
          source: e.source,
          id: null,
          name: "Console",
          change: ev.type === "connected" ? "Connected" : "Disconnected",
          kind: ev.type,
        });
      }
      continue;
    }
    const before = state.get(channelKey(ev.id)) ?? blank(ev.id);
    const after = apply(state, e)!;
    if (e.source === "snapshot") continue;
    const change =
      ev.type === "fader"
        ? `${fmtFader(before.faderDb)} → ${fmtFader(after.faderDb)}`
        : ev.type === "mute"
          ? ev.muted
            ? "Muted"
            : "Unmuted"
          : `Renamed ${before.name} → ${after.name}`;
    rows.push({ seq: e.seq, tMs: e.tMs, source: e.source, id: ev.id, name: before.name, change, kind: ev.type });
  }
  return rows;
}

export interface Lane {
  id: ChannelId;
  name: string;
  /** Fader position over time, starting at 0 ms. */
  fader: { tMs: number; db: number | null }[];
  /** Spans where the channel was muted. */
  mutes: { fromMs: number; toMs: number }[];
  /** One mark per change, for ticks and source markers. */
  marks: { tMs: number; source: ChangeSource; kind: "fader" | "mute" }[];
}

/** One lane per channel that had a fader or mute change after the snapshot. */
export function buildLanes(events: RecordedEvent[], durationMs: number): Lane[] {
  const lanes = new Map<string, Lane>();
  const muteStart = new Map<string, number>();
  const state = new Map<string, ChannelState>();
  const moved = new Set<string>();

  for (const e of events) {
    const ev = e.event;
    if (ev.type !== "fader" && ev.type !== "mute" && ev.type !== "name") continue;
    const key = channelKey(ev.id);
    const s = apply(state, e)!;
    let lane = lanes.get(key);
    if (!lane) {
      lane = { id: ev.id, name: s.name, fader: [], mutes: [], marks: [] };
      lanes.set(key, lane);
    }
    if (ev.type === "name") {
      if (e.source === "snapshot") lane.name = s.name;
      continue;
    }
    if (ev.type === "fader") {
      const last = lane.fader[lane.fader.length - 1];
      if (last && last.tMs === e.tMs) last.db = ev.db;
      else lane.fader.push({ tMs: e.tMs, db: ev.db });
    } else if (ev.muted && !muteStart.has(key)) {
      muteStart.set(key, e.tMs);
    } else if (!ev.muted && muteStart.has(key)) {
      lane.mutes.push({ fromMs: muteStart.get(key)!, toMs: e.tMs });
      muteStart.delete(key);
    }
    if (e.source !== "snapshot") {
      lane.marks.push({ tMs: e.tMs, source: e.source, kind: ev.type });
      moved.add(key);
    }
  }
  for (const [key, from] of muteStart) lanes.get(key)?.mutes.push({ fromMs: from, toMs: durationMs });

  return [...lanes.entries()]
    .filter(([key]) => moved.has(key))
    .map(([, lane]) => lane)
    .sort((a, b) => compareIds(a.id, b.id));
}

// ---- Formatting ----

/** "1:15:02", "0:04:09". Always hours so columns line up. */
export function formatClock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

/** Decimal units like Finder: "1.3 GB", "640 MB", "12 KB". */
export function formatBytes(bytes: number): string {
  if (bytes >= 1e12) return `${(bytes / 1e12).toFixed(1)} TB`;
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  if (bytes >= 1e6) return `${Math.round(bytes / 1e6)} MB`;
  return `${Math.max(1, Math.round(bytes / 1e3))} KB`;
}

export function audioModeLabel(r: Pick<RecordingSummary, "audioMode" | "trackCount">): string {
  const labels: Record<AudioMode, string> = {
    none: "Moves only",
    stereo: "Stereo mix",
    stereoMultitrack: `Stereo + ${r.trackCount} tracks`,
  };
  return labels[r.audioMode];
}

export const SYNC_LABEL: Record<SyncState, string> = {
  localOnly: "Local only",
  pending: "Waiting to upload",
  uploading: "Uploading",
  synced: "Backed up",
  failed: "Upload failed",
};

export const SOURCE_LABEL: Record<ChangeSource, string> = {
  operator: "In the app",
  console: "On the desk",
  assist: "Assist",
  replay: "Replay",
  snapshot: "Start state",
};

function parseServiceDate(serviceDate: string): Date {
  const [y, m, d] = serviceDate.split("-").map(Number);
  return new Date(y, m - 1, d);
}

/** "Sunday, October 4" (with the year when it isn't this year). */
export function serviceDateLabel(serviceDate: string, now = new Date()): string {
  const date = parseServiceDate(serviceDate);
  return date.toLocaleDateString("en-US", {
    weekday: "long",
    month: "long",
    day: "numeric",
    year: date.getFullYear() === now.getFullYear() ? undefined : "numeric",
  });
}

/** "9:00 AM" in local time. */
export function timeOfDay(epochMs: number): string {
  return new Date(epochMs).toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" });
}

/** Default title for a new recording: "Sunday 9:00 AM". */
export function defaultTitle(epochMs: number): string {
  const d = new Date(epochMs);
  return `${d.toLocaleDateString("en-US", { weekday: "long" })} ${timeOfDay(epochMs)}`;
}

/** Local calendar date YYYY-MM-DD. */
export function localDate(epochMs: number): string {
  const d = new Date(epochMs);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

/** Newest service date first; within a date, newest service first. */
export function groupByServiceDate(list: RecordingSummary[]): { date: string; items: RecordingSummary[] }[] {
  const groups = new Map<string, RecordingSummary[]>();
  for (const r of [...list].sort((a, b) => b.startedAt - a.startedAt)) {
    const g = groups.get(r.serviceDate) ?? [];
    g.push(r);
    groups.set(r.serviceDate, g);
  }
  return [...groups.entries()]
    .sort(([a], [b]) => (a < b ? 1 : a > b ? -1 : 0))
    .map(([date, items]) => ({ date, items }));
}

// ---- Disk math (48 kHz, 24-bit WAV; see docs/RECORDINGS.md) ----

const SAMPLE_RATE = 48_000;
const BYTES_PER_SAMPLE = 3;
const HOUR_S = 3600;

/** One mono 24-bit track for an hour: about 0.52 GB. */
export const TRACK_BYTES_PER_HOUR = SAMPLE_RATE * BYTES_PER_SAMPLE * HOUR_S;
/** The stereo main mix for an hour: about 1.0 GB. */
export const STEREO_BYTES_PER_HOUR = TRACK_BYTES_PER_HOUR * 2;

export function bytesPerHour(stereo: boolean, multitrackInputs: number): number {
  return (stereo ? STEREO_BYTES_PER_HOUR : 0) + multitrackInputs * TRACK_BYTES_PER_HOUR;
}

/** Free space the recorder wants before it starts: 3 hours for multitrack, 1 hour for stereo. */
export function requiredFreeBytes(stereo: boolean, multitrackInputs: number): number {
  return Math.max(
    stereo ? STEREO_BYTES_PER_HOUR : 0,
    multitrackInputs > 0 ? 3 * multitrackInputs * TRACK_BYTES_PER_HOUR : 0,
  );
}

/** "16.6 GB" style figure for an hourly estimate. */
export function formatGb(bytes: number): string {
  return `${(bytes / 1e9).toFixed(1)} GB`;
}

export const DAY_MS = 86_400_000;

/** Recordings whose multitracks "Delete multitracks older than N days" would remove. */
export function oldMultitracks(list: RecordingSummary[], days: number, now = Date.now()): RecordingSummary[] {
  return list.filter((r) => r.audioMode === "stereoMultitrack" && r.startedAt < now - days * DAY_MS);
}

/** Track bytes for a recording, estimated from its length (the summary doesn't split them out). */
export function estimatedTrackBytes(r: RecordingSummary): number {
  return Math.round((r.trackCount * TRACK_BYTES_PER_HOUR * r.durationMs) / (HOUR_S * 1000));
}
