// Rule-based observations shown in the Assistant panel today. This is the
// seam where the AI mixing engine plugs in later: it will produce the same
// Insight shape, with an optional action the operator can accept.

import type { MeterFrame } from "./types";
import type { Strip } from "../store/mixer";

export type Severity = "alert" | "warn" | "info";

export interface Insight {
  id: string;
  severity: Severity;
  channel: number;
  title: string;
  detail: string;
}

export function computeInsights(frame: MeterFrame | null, strips: Strip[]): Insight[] {
  if (!frame) return [];
  const out: Insight[] = [];
  for (const m of frame.channels) {
    const strip = strips[m.channel];
    if (!strip) continue;
    const name = strip.name;
    if (m.clipped) {
      out.push({
        id: `clip-${m.channel}`,
        severity: "alert",
        channel: m.channel,
        title: `${name} is clipping`,
        detail: "Lower the preamp gain on the console until peaks sit around -6 dBFS.",
      });
    } else if (m.peakDb > -6) {
      out.push({
        id: `hot-${m.channel}`,
        severity: "warn",
        channel: m.channel,
        title: `${name} is running hot`,
        detail: `Peaks near ${m.peakDb.toFixed(0)} dBFS leave little headroom for a loud moment.`,
      });
    } else if (!strip.muted && m.peakDb < -70 && !/spare/i.test(name)) {
      out.push({
        id: `silent-${m.channel}`,
        severity: "info",
        channel: m.channel,
        title: `No signal on ${name}`,
        detail: "Check the mic, cable or Dante patch if this channel should be live.",
      });
    }
  }
  const rank: Record<Severity, number> = { alert: 0, warn: 1, info: 2 };
  return out.sort((a, b) => rank[a.severity] - rank[b.severity] || a.channel - b.channel);
}
