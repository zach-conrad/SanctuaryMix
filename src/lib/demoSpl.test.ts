import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createDemoSpl } from "./demoSpl";
import type { CalibrationStatus } from "./types";

describe("demo room level", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("prefills history and reports A below C", () => {
    const spl = createDemoSpl();
    spl.start();
    spl.stop();
    const r = spl.reading()!;
    expect(r.calibrated).toBe(false);
    expect(r.leq15m.c).toBeGreaterThan(r.leq15m.a);
    expect(spl.history(300)).toHaveLength(300);
  });

  it("guided calibration listens, then makes the reading match the reference", () => {
    const spl = createDemoSpl();
    spl.start();
    const seen: CalibrationStatus[] = [];
    spl.onCalibration((s) => seen.push(s));
    spl.calibrate("c", 94);
    vi.advanceTimersByTime(6_000);
    spl.stop();
    expect(seen[0].state).toBe("listening");
    const done = seen[seen.length - 1];
    expect(done.state).toBe("done");
    const config = spl.getConfig();
    expect(config.calibrated).toBe(true);
    expect(config.calibration?.referenceDb).toBe(94);
    expect(() => spl.calibrate("a", 200)).toThrow(/between 30 and 140/);
  });

  it("typing an offset or changing the source drops the calibration", () => {
    const spl = createDemoSpl();
    spl.start();
    spl.calibrate("a", 90);
    vi.advanceTimersByTime(6_000);
    expect(spl.getConfig().calibrated).toBe(true);
    spl.setConfig({ ...spl.getConfig(), offsetDb: 110 });
    expect(spl.getConfig()).toMatchObject({ calibrated: false, calibration: null });
    spl.setConfig({ ...spl.getConfig(), source: 3 });
    expect(spl.reading()).toBeNull();
    spl.stop();
  });
});
