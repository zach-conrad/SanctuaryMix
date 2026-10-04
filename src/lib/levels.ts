/** Shared dB helpers for meters and faders (scales from design/GUIDELINES.md). */

type Curve = [db: number, pos: number][];

/** dBFS → meter height. 0 dBFS is the top; zones are fixed by height. */
const METER_CURVE: Curve = [
  [-60, 0], [-48, 0.18], [-30, 0.4], [-18, 0.6], [-12, 0.72], [-6, 0.85], [-3, 0.92], [0, 1],
];

/** Fader dB → travel, unity (0 dB) at 75% like the console. */
const FADER_CURVE: Curve = [
  [-90, 0.01], [-50, 0.12], [-30, 0.28], [-20, 0.4], [-10, 0.55], [0, 0.75], [10, 1],
];

export const FADER_MARKS = [10, 0, -10, -20, -30, -50];

function interp(curve: Curve, x: number, from: 0 | 1, to: 0 | 1): number {
  if (x <= curve[0][from]) return curve[0][to];
  for (let i = 1; i < curve.length; i++) {
    const a = curve[i - 1];
    const b = curve[i];
    if (x <= b[from]) return a[to] + ((x - a[from]) / (b[from] - a[from])) * (b[to] - a[to]);
  }
  return curve[curve.length - 1][to];
}

export function meterPosition(db: number): number {
  return interp(METER_CURVE, db, 0, 1);
}

export function dbToFader(db: number | null): number {
  if (db === null || db <= -90) return 0;
  return interp(FADER_CURVE, db, 0, 1);
}

export function faderToDb(pos: number): number | null {
  if (pos < 0.01) return null;
  return Math.round(interp(FADER_CURVE, pos, 1, 0) * 10) / 10;
}

const MINUS = "−";

/** "−4.5", "0.0", "+2.0", "−∞" (no unit). */
export function formatDb(db: number | null): string {
  if (db === null || db <= -90) return `${MINUS}∞`;
  const r = Math.round(db * 10) / 10;
  const text = Math.abs(r).toFixed(1);
  return r > 0 ? `+${text}` : r < 0 ? `${MINUS}${text}` : text;
}
