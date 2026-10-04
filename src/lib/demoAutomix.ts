// Browser-demo stand-in for crates/automix, so `npm run dev` shows auto-mix
// working without the Rust core. It follows the same rules in simplified form
// (anchors to a reference, the band to the lead vocal, speech ducking, 0.5 dB
// steps, range and unity limits, hands off when a person moves a fader). The
// real guardrails live in Rust; nothing here ships in the app's control path.

import type {
  Adjustment,
  AutoMixConfig,
  AutoMixStatus,
  ChannelMode,
  ChannelRole,
  ChannelStatus,
  MeterFrame,
  Preset,
  RoomFeel,
} from "./types";

const BALANCE_ORDER: ChannelRole[] = [
  "backingVocal", "choir", "kick", "bass", "drums", "keysPads", "pianoOrgan",
  "electricGuitar", "acousticGuitar", "playback", "other",
];

const PRESETS: Preset[] = [
  {
    feel: "fullModern", name: "Full and modern", roomLevel: "92 to 95 dBA", referenceDb: -20,
    description: "Contemporary worship with a full band and vocals on top.",
    balance: [-6, -4, -4, -5, -6, -8, -6, -7, -8, -9, -8],
  },
  {
    feel: "bigLoud", name: "Big and loud", roomLevel: "95 to 97 dBA", referenceDb: -18,
    description: "Concert energy for youth nights and big moments.",
    balance: [-5, -5, -2, -3, -4, -7, -7, -5, -9, -7, -7], adminOnly: true,
  },
  {
    feel: "deepLowEnd", name: "Deep low end", roomLevel: "92 to 95 dBA", referenceDb: -20,
    description: "The modern sound with more kick and bass, at the same loudness.",
    balance: [-6, -4, -2, -3, -6, -7, -6, -7, -8, -8, -8],
  },
  {
    feel: "warmIntimate", name: "Warm and intimate", roomLevel: "82 to 88 dBA", referenceDb: -24,
    description: "Acoustic sets and smaller rooms. Softer, with voices forward.",
    balance: [-7, -4, -9, -8, -10, -5, -4, -10, -4, -12, -8],
  },
  {
    feel: "traditionalChoral", name: "Traditional and choral", roomLevel: "78 to 86 dBA", referenceDb: -26,
    description: "Hymns, choir, piano or organ. The congregation is the lead.",
    balance: [-6, -3, -12, -8, -12, -8, -4, -10, -8, -10, -8],
  },
  {
    feel: "spokenWord", name: "Spoken word", roomLevel: "72 to 78 dBA", referenceDb: -20,
    description: "Sermon, prayer and announcements. Any music sits well under the voice.",
    balance: Array(11).fill(-18), speechOverMusicDb: 18,
  },
].map((p) => ({
  speechReferenceDb: -20,
  speechOverMusicDb: 15,
  adminOnly: false,
  warning: null,
  ...p,
  feel: p.feel as RoomFeel,
})).map((p) => ({
  ...p,
  warning: p.adminOnly ? "Loud enough to tire ears over a long set. Keep worship sets under 30 minutes at this level." : null,
}));

const VOCAL: ChannelRole[] = ["speech", "leadVocal", "backingVocal", "choir"];

export function guessRole(name: string): ChannelRole {
  const n = ` ${name.toLowerCase()} `;
  const has = (words: string[]) => words.some((w) => n.includes(w));
  if (has(["pastor", "preach", "speak", "lapel", "lav", "host", "announce", "podium", "pulpit", "lectern", "handheld", "sermon"])) return "speech";
  if (has(["bgv", " bv", "backing", "harmony"])) return "backingVocal";
  if (has(["choir"])) return "choir";
  if (has(["worship", "lead", " ld ", "vox", "vocal", "cantor"])) return "leadVocal";
  if (has(["kick", " kik", " bd "])) return "kick";
  if (has(["bass"])) return "bass";
  if (has(["snare", "snr", "tom", " hat", " hh ", " oh ", "overhead", "ride", "cymbal", "drum"])) return "drums";
  if (has(["piano", "organ", "grand"])) return "pianoOrgan";
  if (has(["key", "pad", "synth", "rhodes", " kb"])) return "keysPads";
  if (has(["acous", "ac gtr", "agtr", "nylon"])) return "acousticGuitar";
  if (has(["gtr", "guitar", "elec", "egtr"])) return "electricGuitar";
  if (has(["video", "playback", "track", "walk", "media", " pb "])) return "playback";
  return "other";
}

export const DEFAULT_AUTOMIX_CONFIG: AutoMixConfig = {
  feel: "fullModern",
  nudges: { loudnessDb: 0, lowEndDb: 0, vocalPresenceDb: 0 },
  channels: [],
  guardrails: {
    music: { maxStepDb: 0.5, maxRateDbPerSec: 1.5, deadbandDb: 1, maxBoostDb: 6, maxCutDb: 12 },
    speech: { maxStepDb: 1, maxRateDbPerSec: 3, deadbandDb: 1.5, maxBoostDb: 8, maxCutDb: 10 },
    gateDbfs: -50,
    noRaiseAbovePeakDbfs: -3,
  },
};

interface Track {
  level: number | null;
  activeSince: number | null;
  baseline: number | null;
  held: "operator" | "undone" | null;
  converging: boolean;
  atLimit: boolean;
  lastMove: number;
  pending: number | null;
  peak: number;
}

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

export interface DemoAutomixHooks {
  fader(channel: number): number | null;
  name(channel: number): string;
  muted(channel: number): boolean;
  setFader(channel: number, db: number): void;
  status(s: AutoMixStatus): void;
  adjustment(a: Adjustment): void;
}

export function createDemoAutomix(hooks: DemoAutomixHooks) {
  let config: AutoMixConfig = structuredClone(DEFAULT_AUTOMIX_CONFIG);
  let engaged = false;
  let frozen = false;
  const tracks = new Map<number, Track>();
  const log: Adjustment[] = [];

  const track = (ch: number): Track => {
    let t = tracks.get(ch);
    if (!t) {
      t = { level: null, activeSince: null, baseline: null, held: null, converging: false, atLimit: false, lastMove: 0, pending: null, peak: -120 };
      tracks.set(ch, t);
    }
    return t;
  };
  const preset = () => PRESETS.find((p) => p.feel === config.feel) ?? PRESETS[0];
  const record = (a: Omit<Adjustment, "atMs">) => {
    const entry = { ...a, atMs: Date.now() };
    log.unshift(entry);
    log.length = Math.min(log.length, 500);
    hooks.adjustment(entry);
  };
  const note = (kind: Adjustment["kind"], reason: string, channel: number | null = null) =>
    record({ kind, reason, channel, channelName: channel === null ? null : hooks.name(channel), fromDb: null, toDb: null });
  const inUse = (t: Track, now: number, ms: number) => t.activeSince !== null && now - t.activeSince >= ms && t.level !== null;
  const post = (ch: number) => {
    const t = track(ch);
    const f = hooks.fader(ch);
    return t.level === null || f === null ? null : t.level + f;
  };

  function refs(now: number) {
    let speech: [number, number] | null = null;
    let lead: [number, number] | null = null;
    let leadsManaged = false;
    for (const c of config.channels) {
      const t = track(c.channel);
      const usable = (ms: number) => !hooks.muted(c.channel) && t.held !== "undone" && inUse(t, now, ms);
      const level = post(c.channel);
      if (c.role === "leadVocal") leadsManaged = true;
      if (level === null) continue;
      if (c.role === "speech" && usable(300) && (!speech || level > speech[1])) speech = [c.channel, level];
      if (c.role === "leadVocal" && usable(2000) && (!lead || level > lead[1])) lead = [c.channel, level];
    }
    return { speech, lead, leadsManaged };
  }

  function target(role: ChannelRole, r: ReturnType<typeof refs>): number | null {
    const p = preset();
    const n = config.nudges;
    let normal: number | null;
    if (role === "speech" || role === "leadVocal") {
      normal = (role === "speech" ? p.speechReferenceDb : p.referenceDb) + n.loudnessDb;
    } else {
      let offset = p.balance[BALANCE_ORDER.indexOf(role)] ?? -8;
      if (role === "kick" || role === "bass") offset += n.lowEndDb;
      if (!VOCAL.includes(role)) offset -= n.vocalPresenceDb;
      normal = r.lead ? r.lead[1] + offset : r.leadsManaged ? null : p.referenceDb + n.loudnessDb + offset;
    }
    if (r.speech && !VOCAL.includes(role)) {
      const under = r.speech[1] - p.speechOverMusicDb;
      return normal === null ? under : Math.min(normal, under);
    }
    return normal;
  }

  function mode(ch: number, role: ChannelRole, r: ReturnType<typeof refs>, now: number): ChannelMode {
    const t = track(ch);
    if (!engaged) return "off";
    if (t.held === "operator") return "heldByOperator";
    if (t.held === "undone") return "undone";
    if (frozen) return "frozen";
    if (hooks.fader(ch) === null || t.baseline === null) return "waitingForFader";
    if (hooks.muted(ch)) return "muted";
    if (!inUse(t, now, 2000)) return "idle";
    if (target(role, r) === null) return "waitingForLead";
    return t.converging ? (t.atLimit ? "atLimit" : "riding") : "settled";
  }

  function status(): AutoMixStatus {
    const now = performance.now();
    const r = refs(now);
    const live = engaged && !frozen;
    return {
      engaged,
      frozen,
      consoleOnline: true,
      audioOk: true,
      feel: config.feel,
      speechChannel: live && r.speech ? r.speech[0] : null,
      leadChannel: live && r.lead ? r.lead[0] : null,
      channels: config.channels.map<ChannelStatus>((c) => ({
        channel: c.channel,
        name: hooks.name(c.channel),
        role: c.role,
        mode: mode(c.channel, c.role, r, now),
        faderDb: hooks.fader(c.channel),
        baselineDb: track(c.channel).baseline,
        targetDb: target(c.role, r),
        levelDb: inUse(track(c.channel), now, 2000) ? post(c.channel) : null,
      })),
    };
  }

  function tick() {
    const now = performance.now();
    if (engaged && !frozen) {
      const r = refs(now);
      const p = preset();
      for (const c of config.channels) {
        const t = track(c.channel);
        const m = mode(c.channel, c.role, r, now);
        if (m !== "riding" && m !== "atLimit" && m !== "settled") continue;
        const tgt = target(c.role, r);
        const level = post(c.channel);
        const fader = hooks.fader(c.channel);
        if (tgt === null || level === null || fader === null || t.baseline === null) continue;
        const lim = c.role === "speech" ? config.guardrails.speech : config.guardrails.music;
        const error = tgt - level;
        if (t.converging ? Math.abs(error) < 0.5 : Math.abs(error) <= lim.deadbandDb) {
          t.converging = false;
          continue;
        }
        t.converging = true;
        const ceiling = t.baseline > 0 ? 10 : 0;
        const lo = Math.max(t.baseline - lim.maxCutDb, -60);
        const hi = Math.max(lo, Math.min(t.baseline + lim.maxBoostDb, ceiling));
        t.atLimit = (error > 0 && fader >= hi - 1e-3) || (error < 0 && fader <= lo + 1e-3);
        const wanted = clamp(fader + error, lo, hi);
        if (Math.sign(wanted - fader) !== Math.sign(error)) continue;
        if (wanted > fader && t.peak > config.guardrails.noRaiseAbovePeakDbfs) continue;
        const budget = Math.min(lim.maxStepDb, (lim.maxRateDbPerSec * (now - t.lastMove)) / 1000);
        const steps = Math.floor(Math.min(Math.abs(wanted - fader), budget) / 0.5 + 1e-4);
        if (steps < 1) continue;
        const to = Math.round((fader + Math.sign(error) * steps * 0.5) * 10) / 10;
        const up = to > fader;
        const step = Math.abs(to - fader).toFixed(1);
        const reason =
          r.speech && !VOCAL.includes(c.role) && !up
            ? `Stepping back ${step} dB so ${hooks.name(r.speech[0])} stays clear.`
            : r.lead && c.role !== "speech" && c.role !== "leadVocal"
              ? `Was ${Math.abs(error).toFixed(1)} dB too ${up ? "quiet" : "loud"} against ${hooks.name(r.lead[0])}, so it went ${up ? "up" : "down"} ${step} dB.`
              : `Was ${Math.abs(error).toFixed(1)} dB too ${up ? "quiet" : "loud"} for ${p.name.toLowerCase()}, so it went ${up ? "up" : "down"} ${step} dB.`;
        t.lastMove = now;
        t.pending = to;
        hooks.setFader(c.channel, to);
        record({ kind: "auto", channel: c.channel, channelName: hooks.name(c.channel), fromDb: fader, toDb: to, reason });
      }
    }
    hooks.status(status());
  }

  setInterval(tick, 300);

  return {
    presets: () => PRESETS,
    getConfig: () => structuredClone(config),
    setConfig(next: AutoMixConfig) {
      const seen = new Set<number>();
      const channels = next.channels.filter((c) => !seen.has(c.channel) && seen.add(c.channel));
      for (const c of channels) {
        if (!config.channels.some((o) => o.channel === c.channel)) {
          const t = track(c.channel);
          t.baseline = hooks.fader(c.channel);
          t.held = null;
        }
      }
      const n = next.nudges;
      config = {
        ...next,
        channels,
        nudges: { loudnessDb: clamp(n.loudnessDb, -3, 3), lowEndDb: clamp(n.lowEndDb, -3, 3), vocalPresenceDb: clamp(n.vocalPresenceDb, -3, 3) },
      };
      return structuredClone(config);
    },
    engage(on: boolean) {
      if (on === engaged) return;
      engaged = on;
      if (on) {
        for (const c of config.channels) {
          const t = track(c.channel);
          t.baseline = hooks.fader(c.channel);
          t.held = null;
          t.converging = false;
        }
        note("engaged", `Auto-mix started on ${config.channels.length} channels with the ${preset().name.toLowerCase()} room feel.`);
      } else {
        note("disengaged", "Auto-mix stopped. The faders stay where they are.");
      }
      tick();
    },
    freeze() {
      if (frozen) return;
      frozen = true;
      note("frozen", "Frozen. Auto-mix won't move anything until you resume.");
      tick();
    },
    resume() {
      if (!frozen) return;
      frozen = false;
      note("resumed", "Resumed. Auto-mix is riding faders again.");
      tick();
    },
    resumeChannel(ch: number) {
      const t = track(ch);
      if (!t.held) return;
      t.held = null;
      t.baseline = hooks.fader(ch);
      note("channelResumed", "Handed back to auto-mix from where you left it.", ch);
      tick();
    },
    undo(ch: number) {
      const t = track(ch);
      const fader = hooks.fader(ch);
      if (!config.channels.some((c) => c.channel === ch) || t.baseline === null || fader === null) return;
      t.held = "undone";
      if (Math.abs(fader - t.baseline) < 0.25) return;
      t.pending = t.baseline;
      hooks.setFader(ch, t.baseline);
      record({
        kind: "undo", channel: ch, channelName: hooks.name(ch), fromDb: fader, toDb: t.baseline,
        reason: "Back where you set it. Auto-mix is leaving this channel alone until you hand it back.",
      });
      tick();
    },
    undoAll() {
      for (const c of config.channels) this.undo(c.channel);
    },
    status,
    log: (limit = 200) => log.slice(0, limit),
    onMeters(frame: MeterFrame) {
      const now = performance.now();
      const gate = config.guardrails.gateDbfs;
      for (const m of frame.channels) {
        const t = track(m.channel);
        t.peak = Math.max(m.peakDb, t.peak - 0.33);
        if (m.rmsDb > gate) {
          t.activeSince ??= now;
          const p = 10 ** (m.rmsDb / 10);
          const old = t.level === null ? p : 10 ** (t.level / 10);
          t.level = 10 * Math.log10(0.989 * old + 0.011 * p);
        } else {
          t.activeSince = null;
        }
      }
    },
    /** A fader change reported by the (demo) console. Anything we didn't send is a person. */
    onFader(ch: number, db: number | null) {
      const t = track(ch);
      if (t.pending !== null && db !== null && Math.abs(db - t.pending) <= 0.3) {
        t.pending = null;
        return;
      }
      t.pending = null;
      if (!engaged || !config.channels.some((c) => c.channel === ch) || t.held === "operator") return;
      t.held = "operator";
      t.baseline = db;
      t.converging = false;
      record({
        kind: "operatorTookOver", channel: ch, channelName: hooks.name(ch), fromDb: null, toDb: db,
        reason: "You moved this fader, so auto-mix let go of it. Hand it back from Assist when you're ready.",
      });
    },
  };
}
