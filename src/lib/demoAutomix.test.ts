import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createDemoAutomix, DEFAULT_AUTOMIX_CONFIG, guessRole, suggestRole } from "./demoAutomix";
import type { MeterFrame, Sound } from "./types";

describe("guessRole", () => {
  it("matches the Rust guesses for the demo channel names", () => {
    expect(guessRole("Pastor")).toBe("speech");
    expect(guessRole("Worship Ld")).toBe("leadVocal");
    expect(guessRole("BGV 1")).toBe("backingVocal");
    expect(guessRole("OH L")).toBe("drums");
    expect(guessRole("Acous Gtr")).toBe("acousticGuitar");
    expect(guessRole("Elec Gtr")).toBe("electricGuitar");
    expect(guessRole("Pad R")).toBe("keysPads");
    expect(guessRole("Playback L")).toBe("playback");
    expect(guessRole("Ambient L")).toBe("other");
  });
});

describe("demo auto-mix", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  function rig(heard: Record<number, { voice: boolean; sound: Sound | null }> = {}) {
    const faders = new Map<number, number>([[0, -10], [1, -10]]);
    const moves: [number, number][] = [];
    const mix = createDemoAutomix({
      fader: (ch) => faders.get(ch) ?? null,
      name: (ch) => `Ch ${ch + 1}`,
      muted: () => false,
      hear: (ch) => heard[ch] ?? { voice: true, sound: null },
      setFader: (ch, db) => {
        moves.push([ch, db]);
        faders.set(ch, db);
        mix.onFader(ch, db);
      },
      status: () => {},
      adjustment: () => {},
    });
    const frame = (rms: number[]): MeterFrame => ({
      sampleRate: 48000,
      channels: rms.map((r, channel) => ({ channel, rmsDb: r, peakDb: r + 10, clipped: false })),
    });
    const feed = (rms: number[], secs: number) => {
      for (let i = 0; i < secs * 30; i++) {
        mix.onMeters(frame(rms));
        vi.advanceTimersByTime(33);
      }
    };
    return { mix, faders, moves, feed };
  }

  it("rides only picked channels, in console steps, and lets go when a person moves one", () => {
    const { mix, faders, moves, feed } = rig();
    mix.setConfig({ ...DEFAULT_AUTOMIX_CONFIG, channels: [{ channel: 0, role: "leadVocal" }] });
    mix.engage(true);
    feed([-30, -30], 15);
    expect(faders.get(0)).toBe(-4); // +6 dB range limit
    expect(faders.get(1)).toBe(-10);
    for (const [ch, db] of moves) {
      expect(ch).toBe(0);
      expect(Number.isInteger(db * 2)).toBe(true);
    }
    mix.onFader(0, -12);
    faders.set(0, -12);
    feed([-30, -30], 5);
    expect(faders.get(0)).toBe(-12);
    expect(mix.status().channels[0].mode).toBe("heldByOperator");
  });

  it("holds a voice mic that only hears bleed", () => {
    const { mix, faders, feed } = rig({ 0: { voice: false, sound: "music" } });
    mix.setConfig({ ...DEFAULT_AUTOMIX_CONFIG, channels: [{ channel: 0, role: "speech" }] });
    mix.engage(true);
    feed([-30, -30], 10);
    expect(faders.get(0)).toBe(-10);
    expect(mix.status().channels[0].mode).toBe("bleed");
    expect(mix.status().channels[0].heard).toBe("music");
  });
});

describe("suggestRole", () => {
  it("matches the Rust suggestions", () => {
    expect(suggestRole("other", "speech")).toBe("speech");
    expect(suggestRole("backingVocal", "singing")).toBeNull();
    expect(suggestRole("leadVocal", "drums")).toBe("drums");
    expect(suggestRole("keysPads", "organ")).toBeNull();
    expect(suggestRole("bass", "music")).toBeNull();
  });
});
