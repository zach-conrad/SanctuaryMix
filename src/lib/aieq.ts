// AI EQ helpers for the UI and the browser demo: EQ change events, the
// "now → new" lines the design uses, state words, and the report summaries.
// The curve maths lives in eqCurve.ts; the rules live in the Rust core.

import { flatEq, formatGain, formatHz, widthToQ } from "./eqCurve";
import type {
  ChannelEq,
  ChannelRole,
  EqBand,
  EqBandKind,
  EqChange,
  EqChannelStatus,
  EqLogEntry,
  SoundcheckChannel,
} from "./types";

export const MINUS = "−";

/** Band 4 (index 3) is kept free for feedback notches. */
export const RESERVED_BAND = 3;

export function cloneEq(eq: ChannelEq): ChannelEq {
  return { hpf: { ...eq.hpf }, bands: eq.bands.map((b) => ({ ...b })) as ChannelEq["bands"] };
}

/** One console EQ change applied to a channel's EQ. */
export function applyEqChange(eq: ChannelEq | null, change: EqChange): ChannelEq {
  const next = cloneEq(eq ?? flatEq());
  switch (change.param) {
    case "hpfOn":
      next.hpf.on = change.on;
      break;
    case "hpfFreq":
      next.hpf.freqHz = change.hz;
      break;
    case "bandKind":
      if (next.bands[change.band]) next.bands[change.band].kind = change.kind;
      break;
    case "bandFreq":
      if (next.bands[change.band]) next.bands[change.band].freqHz = change.hz;
      break;
    case "bandWidth":
      if (next.bands[change.band]) next.bands[change.band].width = change.width;
      break;
    case "bandGain":
      if (next.bands[change.band]) next.bands[change.band].gainDb = change.db;
      break;
  }
  return next;
}

/** Every parameter of an EQ as change events, as a console sends them on connect. */
export function eqSnapshot(eq: ChannelEq): EqChange[] {
  return diffEq(null, eq);
}

/** The change events that turn `from` into `to` (everything when `from` is null). */
export function diffEq(from: ChannelEq | null, to: ChannelEq): EqChange[] {
  const out: EqChange[] = [];
  if (!from || from.hpf.on !== to.hpf.on) out.push({ param: "hpfOn", on: to.hpf.on });
  if (!from || from.hpf.freqHz !== to.hpf.freqHz) out.push({ param: "hpfFreq", hz: to.hpf.freqHz });
  to.bands.forEach((b, band) => {
    const a = from?.bands[band];
    if (!a || a.kind !== b.kind) out.push({ param: "bandKind", band, kind: b.kind });
    if (!a || a.freqHz !== b.freqHz) out.push({ param: "bandFreq", band, hz: b.freqHz });
    if (!a || a.width !== b.width) out.push({ param: "bandWidth", band, width: b.width });
    if (!a || a.gainDb !== b.gainDb) out.push({ param: "bandGain", band, db: b.gainDb });
  });
  return out;
}

/** "−3.0" with a real minus sign, no unit. */
export function gainNum(db: number): string {
  return formatGain(db).replace(" dB", "");
}

/** "100", "3.2" (the number of formatHz without its unit). */
function hzNum(hz: number): string {
  return formatHz(hz).replace(/ k?Hz$/, "");
}

/** "100 → 140 Hz", "800 Hz → 1.0 kHz": the unit once when both sides share it. */
export function hzChange(from: number, to: number): string {
  const sameUnit = from >= 1000 === to >= 1000;
  return sameUnit ? `${hzNum(from)} → ${formatHz(to)}` : `${formatHz(from)} → ${formatHz(to)}`;
}

/** "0.0 → −3.0 dB". */
export function gainChange(from: number, to: number): string {
  return `${gainNum(from)} → ${formatGain(to)}`;
}

/** The changes between two EQs as the design prints them: "Low cut  100 → 140 Hz", "320 Hz  0.0 → −3.0 dB". */
export function describeChanges(from: ChannelEq, to: ChannelEq): string[] {
  const out: string[] = [];
  if (from.hpf.on !== to.hpf.on || (to.hpf.on && from.hpf.freqHz !== to.hpf.freqHz)) {
    const was = from.hpf.on ? hzNum(from.hpf.freqHz) : "Off";
    out.push(`Low cut  ${to.hpf.on ? `${was} → ${formatHz(to.hpf.freqHz)}` : `${formatHz(from.hpf.freqHz)} → Off`}`);
  }
  to.bands.forEach((b, i) => {
    const a = from.bands[i];
    const moved = Math.abs(a.freqHz - b.freqHz) >= 1;
    const gained = Math.abs(a.gainDb - b.gainDb) >= 0.05;
    const widened = Math.abs(a.width - b.width) >= 0.01;
    if (!moved && !gained && !widened && a.kind === b.kind) return;
    if (gained || moved) out.push(`${formatHz(b.freqHz)}  ${gainChange(a.gainDb, b.gainDb)}`);
    else out.push(`${formatHz(b.freqHz)}  Q ${widthToQ(a.width).toFixed(1)} → ${widthToQ(b.width).toFixed(1)}`);
  });
  return out;
}

/** The width (octaves) for a typed Q, the inverse of widthToQ. */
export function qToWidth(q: number): number {
  let lo = 0.05;
  let hi = 4;
  for (let i = 0; i < 40; i++) {
    const mid = (lo + hi) / 2;
    // Q falls as the width grows.
    if (widthToQ(mid) > q) lo = mid;
    else hi = mid;
  }
  return Math.round(((lo + hi) / 2) * 1000) / 1000;
}

/** "3.2k", "3200", "3.2 kHz", "320 Hz" → Hz. */
export function parseHz(text: string): number | null {
  const t = text.trim().toLowerCase().replace(/\s|hz/g, "");
  const m = /^(\d+(?:\.\d+)?)(k?)$/.exec(t);
  if (!m) return null;
  const v = Number(m[1]) * (m[2] ? 1000 : 1);
  return v >= 20 && v <= 20000 ? v : null;
}

/** "-3", "−3.0 dB", "+2" → dB. */
export function parseDb(text: string): number | null {
  const t = text.trim().replace(MINUS, "-").replace(/\s|db/gi, "");
  if (!/^[+-]?\d+(\.\d+)?$/.test(t)) return null;
  const v = Number(t);
  return v >= -15 && v <= 15 ? v : null;
}

// ---- Bands, for the band table and the graph ----

const BAND_NAMES = ["Low", "Low mid", "High mid", "High"];
const KIND_CAPTION: Record<EqBandKind, string> = {
  bell: "Bell",
  lowShelf: "Shelf",
  highShelf: "Shelf",
  lowPass: "High cut",
  highPass: "Low cut",
};

export function bandName(index: number, band: EqBand): string {
  if (index === 0 && band.kind === "lowShelf") return "Low shelf";
  if (band.kind === "highShelf") return "High shelf";
  return BAND_NAMES[index] ?? `Band ${index + 1}`;
}

export function bandCaption(band: EqBand): string {
  return KIND_CAPTION[band.kind];
}

/** Band 4 holds a notch when it has any gain. */
export function isNotchUsed(eq: ChannelEq): boolean {
  return Math.abs(eq.bands[RESERVED_BAND].gainDb) >= 0.05;
}

// ---- Roles and words ----

export const EQ_GROUPS: { label: string; roles: ChannelRole[] }[] = [
  { label: "Speech", roles: ["speech"] },
  { label: "Vocals", roles: ["leadVocal", "backingVocal", "choir"] },
  {
    label: "Band",
    roles: [
      "kick",
      "bass",
      "drums",
      "keysPads",
      "pianoOrgan",
      "electricGuitar",
      "acousticGuitar",
      "playback",
      "other",
    ],
  },
];

/** 0 speech, 1 vocals, 2 band: the soundcheck order. */
export function roleGroup(role: ChannelRole): number {
  return EQ_GROUPS.findIndex((g) => g.roles.includes(role));
}

export type WordTone = "ai" | "warn" | "manual" | "muted" | "plain";

export interface StateWord {
  word: string;
  tone: WordTone;
  detail: string;
}

/** The state word for a channel in the Assist EQ list. */
export function eqStateWord(ch: EqChannelStatus, enabled: boolean): StateWord {
  if (!enabled || ch.mode === "off") return { word: "Off", tone: "muted", detail: "AI EQ is off for every channel." };
  const notch = ch.notch ? `Notch ${formatHz(ch.notch.hz)}` : null;
  switch (ch.mode) {
    case "consoleOffline":
      return { word: "Console offline", tone: "warn", detail: "The console isn't connected, so nothing changes." };
    case "notSupported":
      return { word: "Can't reach EQ", tone: "warn", detail: "This console's EQ can't be read yet." };
    case "reading":
      return { word: "Reading the desk", tone: "muted", detail: "Reading this channel's EQ from the console." };
    case "frozen":
      return { word: "Frozen", tone: "muted", detail: "Everything is frozen. Resume when you're ready." };
    case "notChecked":
      return { word: "Not checked yet", tone: "warn", detail: "Run the EQ soundcheck to set this channel." };
    case "set":
      return { word: "Set at soundcheck", tone: "plain", detail: "Holding the EQ from the soundcheck." };
    case "keeping":
      return {
        word: notch ? `Keeping tone · ${notch.toLowerCase()}` : "Keeping tone",
        tone: "ai",
        detail: "Small, slow moves keep this voice close to its profile.",
      };
    case "notch":
      return { word: notch ?? "Notch", tone: "ai", detail: "Band 4 is cutting a ringing frequency." };
    case "yours":
      return { word: "Yours", tone: "manual", detail: "You changed this EQ, so AI EQ leaves it alone. Feedback guard still works." };
    case "undone":
      return { word: "Undone", tone: "manual", detail: "Back to its soundcheck EQ. AI EQ holds it until you hand it back." };
  }
}

/** The state word for a channel during the EQ soundcheck. */
export function soundcheckWord(ch: SoundcheckChannel): StateWord {
  switch (ch.state) {
    case "upNext":
      return { word: "Up next", tone: "muted", detail: "Waits for the channel before it." };
    case "listening":
      return { word: "Listening", tone: "ai", detail: "About 20 seconds of sound is enough." };
    case "done":
      return {
        word: `Done · ${ch.changes} ${ch.changes === 1 ? "change" : "changes"}`,
        tone: "ai",
        detail: "Review the change, or apply them all at the top.",
      };
    case "soundsGood":
      return { word: "Sounds good", tone: "plain", detail: "Nothing to change." };
    case "noSound":
      return { word: "No sound yet", tone: "warn", detail: "Nothing came through this mic. Check it's live, then play or speak." };
    case "applied":
      return { word: "Applied", tone: "ai", detail: "On the console now. Undo from the EQ panel." };
    case "skipped":
      return { word: "Skipped", tone: "muted", detail: "Left as it was." };
  }
}

/** What the EQ tag on a strip shows: "ai" when AI EQ changed it live, "manual" when a person took it. */
export function eqTag(ch: EqChannelStatus | undefined, enabled: boolean): "ai" | "manual" | null {
  if (!ch || !enabled) return null;
  if (ch.mode === "yours" || ch.mode === "undone") return "manual";
  if (ch.notch || (ch.toneOffsetDb !== null && Math.abs(ch.toneOffsetDb) >= 0.25)) return "ai";
  return null;
}

/** "Talk" words for the person at each mic during soundcheck. */
export function soundcheckAsk(role: ChannelRole): string {
  if (role === "speech") return "say a few lines";
  if (role === "leadVocal" || role === "backingVocal" || role === "choir") return "sing a verse";
  return "play something typical";
}

// ---- Report ----

const ACTION_WORD: Record<EqLogEntry["action"], string> = {
  soundcheck: "Soundcheck change",
  feedback: "Feedback",
  tone: "Tone keeping",
  undo: "Undone",
  person: "Person took over the EQ",
  handBack: "Handed back to AI EQ",
};

/** One line for a "What changed live" row. */
export function describeEntry(e: EqLogEntry): string {
  if (e.action === "person") return ACTION_WORD.person;
  const changes = e.changes.map((c) => c.replace(/\s{2,}/g, " ")).join(" · ");
  return changes ? `${ACTION_WORD[e.action]} · ${changes}` : ACTION_WORD[e.action];
}

/** The "to" value of a change line in dB, e.g. "250 Hz  −3.0 → −2.0 dB" → −2.0. */
export function changeTargetDb(line: string): { from: number; to: number } | null {
  const m = /([+−-]?\d+(?:\.\d+)?)\s*→\s*([+−-]?\d+(?:\.\d+)?)\s*dB/.exec(line);
  if (!m) return null;
  const n = (s: string) => Number(s.replace(MINUS, "-"));
  return { from: n(m[1]), to: n(m[2]) };
}

export interface VoiceDrift {
  channel: number;
  name: string;
  /** How far tone keeping moved this voice from where the service started, at most, in dB. */
  rangeDb: number;
}

/** Speech mics from the tone-keeping entries: how far each one moved from its profile. */
export function voiceDrift(entries: EqLogEntry[]): VoiceDrift[] {
  const byChannel = new Map<number, { name: string; start: number | null; furthest: number }>();
  const sorted = [...entries].sort((a, b) => a.atMs - b.atMs);
  for (const e of sorted) {
    if (e.action !== "tone") continue;
    const cur = byChannel.get(e.channel) ?? { name: e.channelName, start: null, furthest: 0 };
    for (const line of e.changes) {
      const v = changeTargetDb(line);
      if (!v) continue;
      cur.start ??= v.from;
      cur.furthest = Math.max(cur.furthest, Math.abs(v.to - cur.start));
    }
    byChannel.set(e.channel, cur);
  }
  return [...byChannel.entries()]
    .map(([channel, v]) => ({ channel, name: v.name, rangeDb: Math.round(v.furthest * 2) / 2 }))
    .sort((a, b) => a.channel - b.channel);
}
