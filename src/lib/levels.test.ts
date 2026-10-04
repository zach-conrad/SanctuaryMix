import { describe, expect, it } from "vitest";
import { dbToFader, faderToDb, formatDb, meterPosition } from "./levels";

describe("meterPosition", () => {
  it("clamps to the meter range", () => {
    expect(meterPosition(-80)).toBe(0);
    expect(meterPosition(-60)).toBe(0);
    expect(meterPosition(0)).toBe(1);
    expect(meterPosition(6)).toBe(1);
  });

  it("rises monotonically", () => {
    let last = -1;
    for (let db = -60; db <= 0; db += 0.5) {
      const pos = meterPosition(db);
      expect(pos).toBeGreaterThanOrEqual(last);
      last = pos;
    }
  });
});

describe("fader law", () => {
  it("treats the bottom of travel as -inf", () => {
    expect(faderToDb(0)).toBeNull();
    expect(dbToFader(null)).toBe(0);
  });

  it("puts unity (0 dB) at 75% of travel", () => {
    expect(dbToFader(0)).toBeCloseTo(0.75);
    expect(faderToDb(0.75)).toBe(0);
  });

  it("round-trips dB through fader position", () => {
    for (const db of [-80, -45, -30, -10, 0, 5, 10]) {
      expect(faderToDb(dbToFader(db))).toBeCloseTo(db, 1);
    }
  });
});

describe("formatDb", () => {
  it("formats like a console readout", () => {
    expect(formatDb(null)).toBe("−∞");
    expect(formatDb(-120)).toBe("−∞");
    expect(formatDb(3.14)).toBe("+3.1");
    expect(formatDb(-6)).toBe("−6.0");
    expect(formatDb(0)).toBe("0.0");
  });
});
