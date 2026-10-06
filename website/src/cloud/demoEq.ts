// Sample EQ audit for the demo service, until AI EQ records real entries and
// they sync (draft table: docs/supabase/eq_audit.sql). Times line up with the
// sample recording (150 s, started 9:00 AM).

import type { EqAudit } from "./types";

const START = 1790499600000;
const MIN = 60_000;
const ch = (n: number, name: string) => ({ name, label: `Ch ${n}` });
const ai = { kind: "ai", role: null, name: null } as const;
const jordan = { kind: "person", role: "volunteer", name: "Jordan Lee", where: "app" } as const;
const sam = { kind: "person", role: "engineer", name: "Sam Ortiz", where: "app" } as const;
const desk = { kind: "person", role: null, name: null, where: "desk" } as const;

export const SAMPLE_EQ: EqAudit = {
  entries: [
    {
      seq: 1, tMs: null, at: START - 19 * MIN, channel: ch(1, "Pastor"), action: "soundcheck",
      changes: ["Low cut  off → 100 Hz", "320 Hz  0.0 → −2.0 dB"],
      reason: "Less boomy when he leans in, and no stand thumps.", appliedBy: jordan, by: ai,
    },
    {
      seq: 2, tMs: null, at: START - 18 * MIN, channel: ch(2, "Worship Ld"), action: "soundcheck",
      changes: ["Low cut  off → 90 Hz", "320 Hz  0.0 → −3.0 dB", "3.2 kHz  0.0 → −2.0 dB"],
      reason: "Less boxy, and less harsh on loud notes.", appliedBy: jordan, by: ai,
    },
    {
      seq: 3, tMs: null, at: START - 16 * MIN, channel: ch(3, "BGV 1"), action: "soundcheck",
      changes: ["Low cut  off → 120 Hz"], reason: "Takes the stage rumble out from under the voice.",
      appliedBy: jordan, by: ai,
    },
    {
      seq: 4, tMs: null, at: START - 16 * MIN, channel: ch(4, "BGV 2"), action: "soundcheck",
      changes: ["Low cut  off → 120 Hz"], reason: "Takes the stage rumble out from under the voice.",
      appliedBy: jordan, by: ai,
    },
    {
      seq: 5, tMs: 41_000, at: START + 41_000, channel: ch(3, "BGV 1"), action: "person",
      changes: ["2.5 kHz  0.0 → +2.0 dB"], reason: null, appliedBy: null, by: desk,
    },
    {
      seq: 6, tMs: 74_000, at: START + 74_000, channel: ch(2, "Worship Ld"), action: "undo",
      changes: ["3.2 kHz  −2.0 → 0.0 dB"], reason: null, appliedBy: null, by: jordan,
    },
    {
      seq: 7, tMs: 112_000, at: START + 112_000, channel: ch(1, "Pastor"), action: "feedback",
      changes: ["2.5 kHz  0.0 → −6.0 dB", "Fader  −4.0 → −7.0 dB"],
      reason: "Pastor rang at 2.5 kHz as he walked past the wedge.", appliedBy: null, by: ai,
    },
    {
      seq: 8, tMs: 125_000, at: START + 125_000, channel: ch(1, "Pastor"), action: "tone",
      changes: ["3.2 kHz  0.0 → +0.5 dB"], reason: "His voice was a little duller than last week.",
      appliedBy: null, by: ai,
    },
    {
      seq: 9, tMs: 138_000, at: START + 138_000, channel: ch(3, "BGV 1"), action: "handBack",
      changes: ["2.5 kHz  +2.0 → 0.0 dB"], reason: null, appliedBy: null, by: sam,
    },
  ],
  ideas: [
    {
      channel: ch(1, "Pastor"), title: "Keep the feedback cut", change: "2.5 kHz  −6.0 dB",
      reason: "It rang once today near the wedge.", state: "waiting",
    },
    {
      channel: ch(3, "BGV 1"), title: "A little more presence", change: "2.5 kHz  0.0 → +1.5 dB",
      reason: "Someone added this on the desk today and it sat well.", state: "waiting",
    },
  ],
};
