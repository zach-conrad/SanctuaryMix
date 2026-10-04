import { describe, expect, it } from "vitest";
import { seedRecordings } from "./demoRecordings";
import {
  audioModeLabel,
  buildLanes,
  bytesPerHour,
  describeEvents,
  formatBytes,
  formatClock,
  groupByServiceDate,
  oldMultitracks,
  requiredFreeBytes,
  serviceDateLabel,
  stateAt,
} from "./recordings";
import type { ChangeSource, ConsoleEvent, RecordedEvent, RecordingSummary } from "./types";

const id = (index: number) => ({ kind: "input" as const, index });

function events(list: [number, ChangeSource, ConsoleEvent][]): RecordedEvent[] {
  return list.map(([tMs, source, event], seq) => ({ seq, tMs, source, event }));
}

const SAMPLE = events([
  [0, "snapshot", { type: "name", id: id(0), name: "Pastor" }],
  [0, "snapshot", { type: "fader", id: id(0), db: -5 }],
  [0, "snapshot", { type: "mute", id: id(0), muted: true }],
  [0, "snapshot", { type: "name", id: id(1), name: "Worship Ld" }],
  [0, "snapshot", { type: "fader", id: id(1), db: -2 }],
  [0, "snapshot", { type: "mute", id: id(1), muted: false }],
  [1_000, "operator", { type: "fader", id: id(1), db: 0 }],
  [5_000, "console", { type: "mute", id: id(0), muted: false }],
  [6_000, "assist", { type: "fader", id: id(0), db: -3.5 }],
  [9_000, "operator", { type: "fader", id: id(1), db: null }],
  [9_500, "console", { type: "mute", id: id(0), muted: true }],
]);

describe("stateAt", () => {
  it("is the snapshot at the start", () => {
    expect(stateAt(SAMPLE, 0)).toEqual([
      { id: id(0), name: "Pastor", faderDb: -5, muted: true },
      { id: id(1), name: "Worship Ld", faderDb: -2, muted: false },
    ]);
  });

  it("includes changes up to and including the playhead", () => {
    expect(stateAt(SAMPLE, 999)[1].faderDb).toBe(-2);
    expect(stateAt(SAMPLE, 1_000)[1].faderDb).toBe(0);
    const mid = stateAt(SAMPLE, 7_000);
    expect(mid[0]).toMatchObject({ faderDb: -3.5, muted: false });
  });

  it("ends with every change applied", () => {
    const end = stateAt(SAMPLE, 60_000);
    expect(end[0]).toMatchObject({ faderDb: -3.5, muted: true });
    expect(end[1]).toMatchObject({ faderDb: null });
  });

  it("falls back to console names for channels without a snapshot", () => {
    const s = stateAt(events([[10, "operator", { type: "fader", id: { kind: "dca", index: 1 }, db: -10 }]]), 10);
    expect(s).toEqual([{ id: { kind: "dca", index: 1 }, name: "DCA 2", faderDb: -10, muted: false }]);
  });
});

describe("describeEvents", () => {
  it("lists changes after the snapshot with before and after values", () => {
    const rows = describeEvents(SAMPLE);
    expect(rows.map((r) => r.change)).toEqual([
      "−2.0 dB → 0.0 dB",
      "Unmuted",
      "−5.0 dB → −3.5 dB",
      "0.0 dB → −∞",
      "Muted",
    ]);
    expect(rows[0]).toMatchObject({ name: "Worship Ld", source: "operator", tMs: 1_000 });
    expect(rows[2].source).toBe("assist");
  });
});

describe("buildLanes", () => {
  it("makes one lane per moved channel with mute spans and marks", () => {
    const lanes = buildLanes(SAMPLE, 10_000);
    expect(lanes.map((l) => l.name)).toEqual(["Pastor", "Worship Ld"]);
    expect(lanes[0].mutes).toEqual([
      { fromMs: 0, toMs: 5_000 },
      { fromMs: 9_500, toMs: 10_000 },
    ]);
    expect(lanes[0].fader).toEqual([
      { tMs: 0, db: -5 },
      { tMs: 6_000, db: -3.5 },
    ]);
    expect(lanes[0].marks.map((m) => m.source)).toEqual(["console", "assist", "console"]);
  });

  it("leaves out channels that only appear in the snapshot", () => {
    const quiet = SAMPLE.filter((e) => e.source === "snapshot");
    expect(buildLanes(quiet, 10_000)).toEqual([]);
  });
});

describe("formatting", () => {
  it("formats clocks with hours", () => {
    expect(formatClock(0)).toBe("0:00:00");
    expect(formatClock(4_509_000)).toBe("1:15:09");
  });

  it("formats sizes in decimal units", () => {
    expect(formatBytes(1_296_000_000)).toBe("1.3 GB");
    expect(formatBytes(640_000_000)).toBe("640 MB");
  });

  it("names what was recorded", () => {
    expect(audioModeLabel({ audioMode: "none", trackCount: 0 })).toBe("Moves only");
    expect(audioModeLabel({ audioMode: "stereo", trackCount: 0 })).toBe("Stereo mix");
    expect(audioModeLabel({ audioMode: "stereoMultitrack", trackCount: 32 })).toBe("Stereo + 32 tracks");
  });

  it("labels service dates like a calendar", () => {
    expect(serviceDateLabel("2026-10-04", new Date(2026, 9, 5))).toBe("Sunday, October 4");
    expect(serviceDateLabel("2025-12-24", new Date(2026, 9, 5))).toBe("Wednesday, December 24, 2025");
  });
});

describe("disk math", () => {
  it("matches the spec's estimates", () => {
    expect(bytesPerHour(true, 0) / 1e9).toBeCloseTo(1.04, 2);
    expect(bytesPerHour(false, 32) / 1e9).toBeCloseTo(16.6, 1);
  });

  it("wants 3 hours of room for multitrack and 1 hour for stereo", () => {
    expect(requiredFreeBytes(true, 0)).toBe(bytesPerHour(true, 0));
    expect(requiredFreeBytes(true, 32)).toBe(3 * bytesPerHour(false, 32));
    expect(requiredFreeBytes(false, 0)).toBe(0);
  });
});

describe("demo seed", () => {
  const now = new Date(2026, 9, 4, 15, 0);
  const seeded = seedRecordings(now);
  const summaries = seeded.map((r) => r.detail as RecordingSummary);

  it("groups services by date, newest first", () => {
    const groups = groupByServiceDate(summaries);
    expect(groups[0].date).toBe("2026-10-04");
    expect(groups[0].items.map((r) => r.title)).toEqual(["Sunday 11:00 AM", "Sunday 9:00 AM"]);
    expect(groups.map((g) => g.date)).toEqual([...groups.map((g) => g.date)].sort().reverse());
  });

  it("has assist moves, an interrupted service and one old multitrack", () => {
    expect(seeded.some((r) => r.events.some((e) => e.source === "assist"))).toBe(true);
    expect(summaries.filter((r) => r.status === "interrupted")).toHaveLength(1);
    expect(oldMultitracks(summaries, 30, now.getTime())).toHaveLength(1);
  });

  it("keeps every event inside the recording", () => {
    for (const r of seeded) {
      expect(r.events.every((e) => e.tMs >= 0 && e.tMs <= r.detail.durationMs)).toBe(true);
    }
  });
});
