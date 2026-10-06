// The dLive EQ as maths, for drawing. Identical to crates/tonal/src/model.rs:
// RBJ cookbook biquads at 48 kHz, band width as octaves, 12 dB/octave
// high-pass. Keep the two in step.

import type { ChannelEq, EqBand, Hpf } from "./types";

const FS = 48_000;
const SHELF_Q = Math.SQRT1_2;

/** 60 frequencies, 1/6 octave apart from 20 Hz: the points of EqChannelStatus.spectrum. */
export const SPECTRUM_FREQS: number[] = Array.from({ length: 60 }, (_, i) => 20 * 2 ** (i / 6));

export function widthToQ(width: number): number {
  const n = Math.min(4, Math.max(0.05, width));
  const p = 2 ** n;
  return Math.sqrt(p) / (p - 1);
}

type Kind = EqBand["kind"];

function coeffs(kind: Kind, freq: number, q: number, gainDb: number): number[] {
  const w0 = (2 * Math.PI * Math.min(Math.max(freq, 10), FS * 0.45)) / FS;
  const sin = Math.sin(w0);
  const cos = Math.cos(w0);
  const a = 10 ** (gainDb / 40);
  const alpha = sin / (2 * q);
  let b0: number, b1: number, b2: number, a0: number, a1: number, a2: number;
  switch (kind) {
    case "bell":
      [b0, b1, b2, a0, a1, a2] = [1 + alpha * a, -2 * cos, 1 - alpha * a, 1 + alpha / a, -2 * cos, 1 - alpha / a];
      break;
    case "highPass":
      [b0, b1, b2, a0, a1, a2] = [(1 + cos) / 2, -(1 + cos), (1 + cos) / 2, 1 + alpha, -2 * cos, 1 - alpha];
      break;
    case "lowPass":
      [b0, b1, b2, a0, a1, a2] = [(1 - cos) / 2, 1 - cos, (1 - cos) / 2, 1 + alpha, -2 * cos, 1 - alpha];
      break;
    case "lowShelf": {
      const s = 2 * Math.sqrt(a) * alpha;
      [b0, b1, b2, a0, a1, a2] = [
        a * (a + 1 - (a - 1) * cos + s),
        2 * a * (a - 1 - (a + 1) * cos),
        a * (a + 1 - (a - 1) * cos - s),
        a + 1 + (a - 1) * cos + s,
        -2 * (a - 1 + (a + 1) * cos),
        a + 1 + (a - 1) * cos - s,
      ];
      break;
    }
    case "highShelf": {
      const s = 2 * Math.sqrt(a) * alpha;
      [b0, b1, b2, a0, a1, a2] = [
        a * (a + 1 + (a - 1) * cos + s),
        -2 * a * (a - 1 + (a + 1) * cos),
        a * (a + 1 + (a - 1) * cos - s),
        a + 1 - (a - 1) * cos + s,
        2 * (a - 1 - (a + 1) * cos),
        a + 1 - (a - 1) * cos - s,
      ];
      break;
    }
  }
  return [b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0];
}

function magnitudeDb(c: number[], freq: number): number {
  const w = (2 * Math.PI * freq) / FS;
  const nr = c[0] + c[1] * Math.cos(w) + c[2] * Math.cos(2 * w);
  const ni = -(c[1] * Math.sin(w) + c[2] * Math.sin(2 * w));
  const dr = 1 + c[3] * Math.cos(w) + c[4] * Math.cos(2 * w);
  const di = -(c[3] * Math.sin(w) + c[4] * Math.sin(2 * w));
  return 10 * Math.log10(Math.max(nr * nr + ni * ni, 1e-20) / Math.max(dr * dr + di * di, 1e-20));
}

export function bandResponse(b: EqBand, freq: number): number {
  if (b.kind === "bell" && Math.abs(b.gainDb) < 1e-3) return 0;
  const q = b.kind === "bell" ? widthToQ(b.width) : SHELF_Q;
  return magnitudeDb(coeffs(b.kind, b.freqHz, q, b.gainDb), freq);
}

export function hpfResponse(hpf: Hpf, freq: number): number {
  if (!hpf.on) return 0;
  return magnitudeDb(coeffs("highPass", hpf.freqHz, SHELF_Q, 0), freq);
}

export function response(eq: ChannelEq, freq: number): number {
  return hpfResponse(eq.hpf, freq) + eq.bands.reduce((sum, b) => sum + bandResponse(b, freq), 0);
}

/** Log-spaced frequencies from 20 Hz to 20 kHz, for drawing a curve. */
export function curveFreqs(points = 160): number[] {
  return Array.from({ length: points }, (_, i) => 20 * 1000 ** (i / (points - 1)));
}

/** "320 Hz", "3.2 kHz". */
export function formatHz(hz: number): string {
  if (hz >= 1000) {
    const k = hz / 1000;
    return `${k >= 10 ? k.toFixed(0) : k.toFixed(1)} kHz`;
  }
  return `${Math.round(hz)} Hz`;
}

/** "−3.0 dB" with a real minus sign, "+2.0 dB", "0.0 dB". */
export function formatGain(db: number): string {
  const v = Math.abs(db) < 0.05 ? 0 : db;
  const s = Math.abs(v).toFixed(1);
  return `${v < 0 ? "−" : v > 0 ? "+" : ""}${s} dB`;
}

/** Q for display, one decimal. */
export function formatQ(width: number): string {
  return widthToQ(width).toFixed(1);
}

/** A flat EQ laid out like a fresh dLive input (matches ChannelEq::default). */
export function flatEq(): ChannelEq {
  return {
    hpf: { on: false, freqHz: 80 },
    bands: [
      { kind: "bell", freqHz: 100, width: 1, gainDb: 0 },
      { kind: "bell", freqHz: 500, width: 1, gainDb: 0 },
      { kind: "bell", freqHz: 2000, width: 1, gainDb: 0 },
      { kind: "bell", freqHz: 8000, width: 1, gainDb: 0 },
    ],
  };
}
