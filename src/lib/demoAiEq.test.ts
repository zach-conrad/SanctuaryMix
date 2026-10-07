import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createDemoAiEq, SOUNDCHECK_SECS } from "./demoAiEq";
import type { AiEqStatus, ChannelRole, EqChange, EqLogEntry, ManagedChannel, Role } from "./types";

const NAMES = ["Pastor", "Worship Ld", "BGV 1", "BGV 2", "Acous Gtr"];
const PICKS: ManagedChannel[] = [
  { channel: 0, role: "speech" },
  { channel: 1, role: "leadVocal" },
  { channel: 2, role: "backingVocal" },
  { channel: 3, role: "backingVocal" },
  { channel: 4, role: "acousticGuitar" as ChannelRole },
];

describe("demo AI EQ", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  function rig(opts: { inService?: boolean; role?: Role } = {}) {
    const desk: [number, EqChange][] = [];
    const log: EqLogEntry[] = [];
    const faders = new Map<number, number>(PICKS.map((p) => [p.channel, -5]));
    let last: AiEqStatus | null = null;
    const state = { inService: opts.inService ?? false, role: opts.role ?? ("admin" as Role) };
    const eq = createDemoAiEq({
      name: (ch) => NAMES[ch] ?? `Ch ${ch + 1}`,
      picked: () => PICKS,
      consoleOnline: () => true,
      inService: () => state.inService,
      allowed: () => true,
      role: () => state.role,
      fader: (ch) => faders.get(ch) ?? null,
      setFader: (ch, db) => faders.set(ch, db),
      deskEq: (ch, change) => desk.push([ch, change]),
      status: (s) => (last = s),
      log: (e) => log.push(e),
      recording: async () => ({ startedAt: Date.UTC(2026, 9, 4, 14), durationMs: 75 * 60_000, title: "Sunday 9:00 AM" }),
    });
    return { eq, desk, log, faders, state, last: () => last ?? eq.status() };
  }

  it("listens in role order, proposes, and applies only on tap", () => {
    const { eq, desk, log, last } = rig();
    eq.soundcheckStart();
    const order = last().soundcheck!.channels.map((c) => c.channel);
    expect(order).toEqual([0, 1, 2, 3, 4]);
    expect(last().soundcheck!.channels[0].state).toBe("listening");

    vi.advanceTimersByTime((SOUNDCHECK_SECS + 1) * 1000);
    const pastor = last().channels.find((c) => c.channel === 0)!;
    expect(pastor.proposal?.changes).toEqual(["250 Hz  −2.5 → −4.0 dB"]);
    expect(desk).toEqual([]); // nothing reaches the desk before Apply

    // The backing vocals listen together; the unplugged acoustic ends as "No sound yet".
    vi.advanceTimersByTime(80_000);
    const states = Object.fromEntries(last().soundcheck!.channels.map((c) => [c.channel, c.state]));
    expect(states).toEqual({ 0: "done", 1: "done", 2: "done", 3: "done", 4: "noSound" });
    expect(last().soundcheck!.running).toBe(false);

    eq.applyAll();
    expect(last().channels.every((c) => c.proposal === null)).toBe(true);
    expect(desk.some(([ch, c]) => ch === 1 && c.param === "bandGain")).toBe(true);
    expect(log.filter((e) => e.action === "soundcheck")).toHaveLength(4);
  });

  it("flips Before and After, and Undo offers the suggestion again", () => {
    const { eq, last } = rig();
    eq.soundcheckStart();
    eq.fastForward(45);
    eq.apply(1);
    expect(last().channels[1].eq!.bands[1].gainDb).toBe(-3);
    eq.compare(1, "before");
    expect(last().channels[1].eq!.bands[1].gainDb).toBe(0);
    expect(last().compare).toEqual({ channel: 1, side: "before" });
    eq.compare(1, "after");
    eq.undo(1);
    expect(last().channels[1].eq!.bands[1].gainDb).toBe(0);
    expect(last().channels[1].proposal?.title).toBe("Less boxy, less harsh on loud notes");
  });

  it("refuses soundcheck changes mid-service and setup changes from volunteers", () => {
    const { eq } = rig({ inService: true, role: "volunteer" });
    expect(() => eq.soundcheckStart()).toThrow();
    expect(() => eq.keepMyEq()).toThrow();
    expect(() => eq.ringOutStart()).toThrow();
    expect(() => eq.setConfig({ enabled: true, toneKeeping: false, tap: "beforeEq" })).toThrow();
    expect(eq.setConfig({ enabled: false, toneKeeping: true, tap: "beforeEq" }).enabled).toBe(false);
  });

  it("notches feedback on band 4, and falls back to the fader when band 4 is busy", () => {
    const { eq, faders, last } = rig();
    eq.triggerFeedback(0, 2500);
    let fb = last().feedback!;
    expect(fb).toMatchObject({ channel: 0, hz: 2500, notchDb: -6, faderCutDb: 3, countToday: 1 });
    expect(last().channels[0].eq!.bands[3]).toMatchObject({ freqHz: 2500, gainDb: -6 });
    expect(faders.get(0)).toBe(-8);

    eq.triggerFeedback(0, 630);
    fb = last().feedback!;
    expect(fb.notchDb).toBeNull();
    expect(fb.countToday).toBe(2);
    expect(last().channels[0].eq!.bands[3].freqHz).toBe(2500);

    eq.undo(0);
    expect(last().channels[0].notch).toBeNull();
    expect(last().channels[0].mode).toBe("undone");
    expect(last().feedback).toBeNull();
  });

  it("stops every automatic move while frozen", () => {
    const { eq, last } = rig();
    eq.freeze();
    eq.triggerFeedback(0, 2500);
    expect(last().feedback).toBeNull();
    expect(last().channels[0].mode).toBe("frozen");
  });

  it("makes a person's edit theirs until they hand it back", () => {
    const { eq, log, last } = rig();
    const mine = structuredClone(last().channels[2].eq!);
    mine.bands[1].gainDb = -4;
    eq.setEq(2, mine);
    expect(last().channels[2].mode).toBe("yours");
    expect(log[log.length - 1]?.action).toBe("person");
    eq.handBack(2);
    expect(last().channels[2].mode).toBe("set");
  });

  it("has ideas and an audit for a recorded service", async () => {
    const { eq } = rig();
    const ideas = await eq.ideas("rec-1");
    expect(ideas.map((i) => i.title)).toEqual(["Keep the Pastor notch", "Warm up BGV 1", "Less rumble on Acous Gtr"]);
    expect(eq.setIdea(ideas[0].id, "kept")[0].state).toBe("kept");
    const audit = await eq.audit("rec-1");
    expect(audit.entries.map((e) => e.action)).toContain("feedback");
    expect(audit.entries.every((e, i, all) => i === 0 || all[i - 1].tMs! <= e.tMs!)).toBe(true);
  });
});
