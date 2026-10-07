import { describe, expect, it } from "vitest";
import { bandResponse, formatGain, formatHz, hpfResponse, widthToQ } from "./eqCurve";

// Same expectations as crates/tonal/src/model.rs, so the drawn curve matches the engine.
describe("eqCurve", () => {
  it("peaks a bell at its gain", () => {
    const b = { kind: "bell" as const, freqHz: 1000, width: 1, gainDb: -6 };
    expect(bandResponse(b, 1000)).toBeCloseTo(-6, 1);
    expect(Math.abs(bandResponse(b, 100))).toBeLessThan(0.3);
  });

  it("uses octave widths", () => {
    expect(widthToQ(1)).toBeCloseTo(1.414, 2);
  });

  it("rolls the high-pass off at 12 dB per octave", () => {
    const hpf = { on: true, freqHz: 100 };
    expect(hpfResponse(hpf, 100)).toBeCloseTo(-3, 0);
    expect(hpfResponse(hpf, 25)).toBeLessThan(-22);
    expect(hpfResponse({ on: false, freqHz: 100 }, 30)).toBe(0);
  });

  it("formats like the design", () => {
    expect(formatHz(320)).toBe("320 Hz");
    expect(formatHz(3200)).toBe("3.2 kHz");
    expect(formatGain(-3)).toBe("−3.0 dB");
    expect(formatGain(0.01)).toBe("0.0 dB");
  });
});
