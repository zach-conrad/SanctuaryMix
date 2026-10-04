/** Shared dB helpers for meters and faders. */

export const METER_FLOOR_DB = -60;
export const FADER_MAX_DB = 10;

/** dBFS to a 0..1 meter height. */
export function meterPosition(db: number): number {
  return Math.max(0, Math.min(1, (db - METER_FLOOR_DB) / -METER_FLOOR_DB));
}

/**
 * Fader travel (0..1) to dB, shaped like a console fader: the top 75% covers
 * -30..+10 dB so the useful range gets most of the throw.
 */
export function faderToDb(pos: number): number | null {
  if (pos <= 0.005) return null;
  if (pos >= 0.25) return -30 + ((pos - 0.25) / 0.75) * 40;
  return -90 + (pos / 0.25) * 60;
}

export function dbToFader(db: number | null): number {
  if (db === null || db <= -90) return 0;
  if (db >= -30) return 0.25 + ((Math.min(db, FADER_MAX_DB) + 30) / 40) * 0.75;
  return ((db + 90) / 60) * 0.25;
}

export function formatDb(db: number | null): string {
  if (db === null || db <= -90) return "-∞";
  const r = Math.round(db * 10) / 10;
  return `${r > 0 ? "+" : ""}${r.toFixed(1)}`;
}
