// Seed data for the browser demo's Mix Manager: a few past services with
// believable control moves. Deterministic so screenshots stay stable.

import { STEREO_BYTES_PER_HOUR, TRACK_BYTES_PER_HOUR, localDate } from "./recordings";
import type { AudioMode, ChangeSource, ChannelId, RecordedEvent, RecordingDetail, RecordingStatusKind } from "./types";

export const DEMO_NAMES = [
  "Pastor", "Worship Ld", "BGV 1", "BGV 2", "Kick", "Snare", "Hat", "Tom 1", "Tom 2",
  "OH L", "OH R", "Bass DI", "Elec Gtr", "Acous Gtr", "Keys L", "Keys R", "Pad L", "Pad R",
  "Choir L", "Choir R", "Handheld 1", "Handheld 2", "Video L", "Video R", "Playback L",
  "Playback R", "Lapel 1", "Lapel 2", "Ambient L", "Ambient R", "Spare 1", "Spare 2",
];

/** Where a typical Sunday leaves each fader; null is fully down. */
export const DEMO_FADERS: (number | null)[] = [
  -5, -2, -6, -6.5, -4, -6, -12, -8, -8, -10, -10, -3, -6, -7, -6, -6, -14, -14,
  -10, -10, -8, -8, -5, -5, -8, -8, -6, -6, -20, -20, null, null,
];

/** Muted at the top of a service: pastor (walk-in), spare mics, video. */
export const DEMO_MUTED = new Set([0, 20, 21, 22, 23, 26, 27, 30, 31]);

const input = (index: number): ChannelId => ({ kind: "input", index });

function rng(seed: number) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const round = (db: number) => Math.round(db * 2) / 2;

/** Snapshot rows at t=0 for the given console state. */
export function snapshotEvents(
  names: string[],
  faders: (number | null)[],
  muted: Set<number> | ((i: number) => boolean),
): RecordedEvent[] {
  const isMuted = typeof muted === "function" ? muted : (i: number) => muted.has(i);
  const out: RecordedEvent[] = [];
  names.forEach((name, i) => {
    const id = input(i);
    out.push({ seq: out.length, tMs: 0, source: "snapshot", event: { type: "name", id, name } });
    out.push({ seq: out.length, tMs: 0, source: "snapshot", event: { type: "fader", id, db: faders[i] ?? null } });
    out.push({ seq: out.length, tMs: 0, source: "snapshot", event: { type: "mute", id, muted: isMuted(i) } });
  });
  return out;
}

type Move = { tMs: number; source: ChangeSource; ch: number } & (
  | { kind: "fader"; db: number | null }
  | { kind: "mute"; muted: boolean }
);

/** The moves of one service, shaped like a real Sunday or Wednesday night. */
function serviceMoves(durationMs: number, seed: number, wednesday: boolean): Move[] {
  const r = rng(seed);
  const moves: Move[] = [];
  const min = (m: number) => Math.round(((m * durationMs) / (75 * 60_000)) * 60_000);
  const faders = [...DEMO_FADERS];
  const fader = (m: number, ch: number, db: number | null, source: ChangeSource = "operator") => {
    faders[ch] = db;
    moves.push({ tMs: min(m), source, ch, kind: "fader", db });
  };
  const ride = (m: number, ch: number, by: number, source: ChangeSource = "operator") =>
    fader(m, ch, round(Math.min(4, (faders[ch] ?? -30) + by)), source);
  const mute = (m: number, ch: number, muted: boolean, source: ChangeSource = "console") =>
    moves.push({ tMs: min(m), source, ch, kind: "mute", muted });

  if (wednesday) {
    // Small band: pastor lapel, worship leader, acoustic, keys.
    mute(3, 24, true);
    mute(3, 25, true);
    for (let m = 4; m < 26; m += 2 + r() * 3) ride(m, 1, (r() - 0.5) * 3);
    ride(9, 13, 2);
    ride(17, 13, -2);
    mute(27, 0, false);
    fader(27.2, 14, -18);
    fader(27.2, 15, -18);
    for (let m = 29; m < 70; m += 3 + r() * 4) ride(m, 0, (r() - 0.5) * 2.5);
    ride(41, 0, 1.5, "assist");
    ride(55, 0, -1, "assist");
    mute(71, 0, true);
    fader(71, 14, -6);
    fader(71, 15, -6);
    return moves.sort((a, b) => a.tMs - b.tMs);
  }

  // Walk-in music, then the worship set.
  mute(4, 24, true);
  mute(4, 25, true);
  for (let m = 4.5; m < 24; m += 0.8 + r() * 1.6) ride(m, 1, (r() - 0.5) * 3);
  for (let m = 6; m < 24; m += 3 + r() * 3) ride(m, 2, (r() - 0.5) * 2.5);
  ride(11.5, 12, 3);
  ride(13, 12, -3);
  ride(15, 18, 4);
  ride(15, 19, 4);
  ride(22, 18, -4);
  ride(22, 19, -4);
  ride(19, 2, 1.5, "assist");

  // Announcements and a video.
  mute(24, 20, false);
  ride(24.3, 20, 2);
  mute(26.5, 22, false);
  mute(26.5, 23, false);
  mute(29.5, 22, true);
  mute(29.5, 23, true);
  mute(30.5, 20, true);

  // Sermon: band down, pastor up, small rides with a few from Assist.
  for (const ch of [4, 5, 6, 12]) mute(31, ch, true);
  mute(31, 0, false);
  fader(31.2, 0, -3);
  for (let m = 33; m < 66; m += 2 + r() * 3) ride(m, 0, (r() - 0.5) * 2);
  ride(38, 0, 1.5, "assist");
  ride(47, 0, -1, "assist");
  ride(53, 0, 1, "assist");
  ride(60.5, 0, 1.5, "assist");
  fader(63, 14, -16);
  fader(63, 15, -16);

  // Closing song.
  for (const ch of [4, 5, 6, 12]) mute(68, ch, false);
  fader(68, 14, -6);
  fader(68, 15, -6);
  mute(70, 0, true);
  for (let m = 68.5; m < 73.5; m += 1 + r()) ride(m, 1, (r() - 0.5) * 2.5);
  mute(73.8, 24, false);
  mute(73.8, 25, false);
  return moves.sort((a, b) => a.tMs - b.tMs);
}

export interface DemoRecording {
  detail: RecordingDetail;
  events: RecordedEvent[];
}

interface Seed {
  daysAgo: number;
  hour: number;
  minute: number;
  minutes: number;
  audioMode: AudioMode;
  status?: RecordingStatusKind;
  /** Interrupted recordings stop early; this is where. */
  cutAtMinutes?: number;
  notes?: string;
  wednesday?: boolean;
}

/** The most recent Sunday whose 11:00 service has finished. */
function lastSunday(now: Date): Date {
  const d = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  d.setDate(d.getDate() - d.getDay());
  if (d.getTime() + 12.5 * 3_600_000 > now.getTime()) d.setDate(d.getDate() - 7);
  return d;
}

export function buildRecording(
  id: string,
  startedAt: number,
  durationMs: number,
  opts: {
    title: string;
    audioMode: AudioMode;
    status: RecordingStatusKind;
    events: RecordedEvent[];
    notes?: string;
    trackCount?: number;
  },
): DemoRecording {
  const hours = durationMs / 3_600_000;
  const trackCount = opts.audioMode === "stereoMultitrack" ? (opts.trackCount ?? 32) : 0;
  const mixBytes = opts.audioMode === "none" ? 0 : Math.round(STEREO_BYTES_PER_HOUR * hours);
  const trackBytes = Math.round(TRACK_BYTES_PER_HOUR * hours);
  const eventsBytes = opts.events.length * 120;
  const files: RecordingDetail["files"] = [
    { kind: "manifest", channel: null, relPath: "manifest.json", bytes: 2_048 },
    { kind: "events", channel: null, relPath: "events.jsonl", bytes: eventsBytes },
  ];
  if (mixBytes) files.unshift({ kind: "mix", channel: null, relPath: "mix.wav", bytes: mixBytes });
  for (let i = 0; i < trackCount; i++) {
    files.push({
      kind: "track",
      channel: i,
      relPath: `tracks/in-${String(i + 1).padStart(2, "0")}.wav`,
      bytes: trackBytes,
    });
  }
  const channelNames = opts.events
    .filter((e) => e.source === "snapshot" && e.event.type === "name")
    .map((e) => (e.event.type === "name" ? { id: e.event.id, name: e.event.name } : null))
    .filter((x): x is { id: ChannelId; name: string } => x !== null);

  return {
    events: opts.events,
    detail: {
      id,
      orgId: null,
      createdBy: "local",
      title: opts.title,
      serviceDate: localDate(startedAt),
      startedAt,
      endedAt: opts.status === "recording" ? null : startedAt + durationMs,
      durationMs,
      status: opts.status,
      consoleModel: "dlive",
      sampleRate: opts.audioMode === "none" ? null : 48_000,
      audioMode: opts.audioMode,
      mixChannels: opts.audioMode === "none" ? null : [62, 63],
      trackCount,
      bytesOnDisk: files.reduce((n, f) => n + f.bytes, 0),
      notes: opts.notes ?? "",
      eventCount: opts.events.length,
      syncState: "localOnly",
      files,
      channelNames,
    },
  };
}

export function seedRecordings(now = new Date()): DemoRecording[] {
  const sunday = lastSunday(now);
  const seeds: Seed[] = [
    { daysAgo: 0, hour: 11, minute: 0, minutes: 76.4, audioMode: "stereoMultitrack", notes: "Guest worship leader. Multitrack for a virtual soundcheck on Thursday." },
    { daysAgo: 0, hour: 9, minute: 0, minutes: 73.1, audioMode: "stereo" },
    { daysAgo: 4, hour: 19, minute: 0, minutes: 62.5, audioMode: "none", wednesday: true, notes: "Midweek study. Audio not patched yet, moves only." },
    { daysAgo: 7, hour: 11, minute: 0, minutes: 78.2, audioMode: "stereo" },
    { daysAgo: 7, hour: 9, minute: 0, minutes: 75, audioMode: "stereo", status: "interrupted", cutAtMinutes: 41.3, notes: "Power blip during the sermon. The recording stops at 41 minutes." },
    { daysAgo: 14, hour: 11, minute: 0, minutes: 74.6, audioMode: "stereo" },
    { daysAgo: 35, hour: 11, minute: 0, minutes: 77.8, audioMode: "stereoMultitrack", notes: "Baptism Sunday." },
  ];

  return seeds.map((s, n) => {
    const day = new Date(sunday);
    day.setDate(day.getDate() - s.daysAgo);
    day.setHours(s.hour, s.minute + (n % 3), 12 + n * 7, 0);
    const startedAt = day.getTime();
    const plannedMs = Math.round(s.minutes * 60_000);
    const durationMs = s.cutAtMinutes ? Math.round(s.cutAtMinutes * 60_000) : plannedMs;
    const snapshot = snapshotEvents(DEMO_NAMES, DEMO_FADERS, DEMO_MUTED);
    const moves = serviceMoves(plannedMs, 1000 + n * 17, !!s.wednesday).filter((m) => m.tMs <= durationMs);
    const events: RecordedEvent[] = [...snapshot];
    for (const m of moves) {
      events.push({
        seq: events.length,
        tMs: m.tMs,
        source: m.source,
        event:
          m.kind === "fader"
            ? { type: "fader", id: input(m.ch), db: m.db }
            : { type: "mute", id: input(m.ch), muted: m.muted },
      });
    }
    const title = s.wednesday
      ? "Wednesday night"
      : `Sunday ${s.hour > 12 ? s.hour - 12 : s.hour}:00 ${s.hour >= 12 ? "PM" : "AM"}`;
    return buildRecording(`demo-${startedAt.toString(36)}`, startedAt, durationMs, {
      title,
      audioMode: s.audioMode,
      status: s.status ?? "complete",
      events,
      notes: s.notes,
    });
  });
}

