import { describe, expect, it } from "vitest";
import { createDemoSpl } from "./demoSpl";

describe("demo room level", () => {
  it("prefills history and reports A below C", () => {
    const spl = createDemoSpl();
    spl.start();
    spl.stop();
    const r = spl.reading()!;
    expect(r.calibrated).toBe(false);
    expect(r.leq15m.c).toBeGreaterThan(r.leq15m.a);
    expect(spl.history(300)).toHaveLength(300);
  });

  it("calibration makes the slow level read the reference", () => {
    const spl = createDemoSpl();
    spl.start();
    spl.stop();
    const config = spl.calibrate("c", 94);
    expect(config.calibrated).toBe(true);
    expect(spl.reading()!.slow.c).toBeCloseTo(94, 5);
    expect(() => spl.calibrate("a", 200)).toThrow(/between 30 and 140/);
  });

  it("a new source starts over and turning it off stops readings", () => {
    const spl = createDemoSpl();
    spl.start();
    spl.stop();
    spl.setConfig({ ...spl.getConfig(), source: 3 });
    expect(spl.reading()).toBeNull();
    expect(spl.history(60)).toHaveLength(0);
    spl.setConfig({ ...spl.getConfig(), source: null });
    expect(spl.reading()).toBeNull();
  });
});
