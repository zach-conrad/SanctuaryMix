import { describe, expect, it } from "vitest";
import {
  applyEqChange,
  describeChanges,
  diffEq,
  eqSnapshot,
  eqStateWord,
  eqTag,
  parseDb,
  parseHz,
  qToWidth,
  voiceDrift,
} from "./aieq";
import { flatEq, widthToQ } from "./eqCurve";
import type { ChannelEq, EqChannelStatus, EqLogEntry } from "./types";

const status = (patch: Partial<EqChannelStatus>): EqChannelStatus => ({
  channel: 0,
  name: "Pastor",
  role: "speech",
  mode: "set",
  eq: flatEq(),
  baseline: flatEq(),
  proposal: null,
  notch: null,
  toneOffsetDb: null,
  spectrum: null,
  heardSecs: 0,
  differsFromProfile: false,
  ...patch,
});

describe("EQ change events", () => {
  it("rebuilds an EQ from the console's snapshot and diffs", () => {
    const eq: ChannelEq = flatEq();
    eq.hpf = { on: true, freqHz: 140 };
    eq.bands[1] = { kind: "bell", freqHz: 320, width: 1, gainDb: -3 };
    const rebuilt = eqSnapshot(eq).reduce<ChannelEq | null>((e, c) => applyEqChange(e, c), null);
    expect(rebuilt).toEqual(eq);
    const next = applyEqChange(eq, { param: "bandGain", band: 2, db: -2 });
    expect(diffEq(eq, next)).toEqual([{ param: "bandGain", band: 2, db: -2 }]);
  });
});

describe("describeChanges", () => {
  it("prints changes the way the design does", () => {
    const from = flatEq();
    from.hpf = { on: true, freqHz: 100 };
    from.bands[1].freqHz = 320;
    const to: ChannelEq = structuredClone(from);
    to.hpf.freqHz = 140;
    to.bands[1].gainDb = -3;
    expect(describeChanges(from, to)).toEqual(["Low cut  100 → 140 Hz", "320 Hz  0.0 → −3.0 dB"]);
    expect(describeChanges(from, from)).toEqual([]);
  });
});

describe("typing values", () => {
  it("reads frequencies, gains and Q", () => {
    expect(parseHz("3.2k")).toBe(3200);
    expect(parseHz("320 Hz")).toBe(320);
    expect(parseHz("5")).toBeNull();
    expect(parseDb("−3.0 dB")).toBe(-3);
    expect(parseDb("+2")).toBe(2);
    expect(parseDb("loud")).toBeNull();
    expect(widthToQ(qToWidth(1.4))).toBeCloseTo(1.4, 2);
  });
});

describe("state words", () => {
  it("gives every state a word", () => {
    expect(eqStateWord(status({ mode: "notChecked" }), true)).toMatchObject({ word: "Not checked yet", tone: "warn" });
    expect(eqStateWord(status({ mode: "notch", notch: { hz: 2500, gainDb: -6, atMs: 0 } }), true).word).toBe(
      "Notch 2.5 kHz",
    );
    expect(eqStateWord(status({ mode: "set" }), false).word).toBe("Off");
  });

  it("tags a strip only for live AI changes or a person's takeover", () => {
    expect(eqTag(status({}), true)).toBeNull();
    expect(eqTag(status({ notch: { hz: 2500, gainDb: -6, atMs: 0 } }), true)).toBe("ai");
    expect(eqTag(status({ mode: "yours" }), true)).toBe("manual");
    expect(eqTag(status({ mode: "yours" }), false)).toBeNull();
  });
});

describe("voiceDrift", () => {
  it("measures how far tone keeping moved each voice", () => {
    const entry = (atMs: number, line: string): EqLogEntry => ({
      atMs,
      channel: 0,
      channelName: "Pastor",
      action: "tone",
      changes: [line],
      reason: null,
      by: { kind: "ai", role: null, name: null, userId: null, where: null },
      appliedBy: null,
    });
    const drift = voiceDrift([entry(2, "250 Hz  −2.5 → −2.0 dB"), entry(1, "250 Hz  −3.0 → −2.5 dB")]);
    expect(drift).toEqual([{ channel: 0, name: "Pastor", rangeDb: 1 }]);
  });
});
