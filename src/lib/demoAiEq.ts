// Browser-demo stand-in for crates/tonal (AI EQ), so `npm run dev` shows the
// EQ soundcheck, feedback guard and tone keeping without the Rust core. It
// follows the behaviour in simplified, scripted form: proposals are written
// per channel rather than heard, and the limits are the core's defaults. The
// real listening, limits and logging live in Rust; nothing here ships in the
// app's control path.

import { cloneEq, describeChanges, diffEq, MINUS, RESERVED_BAND, roleGroup } from "./aieq";
import { flatEq, formatGain, formatHz, SPECTRUM_FREQS } from "./eqCurve";
import type {
  AiEqConfig,
  AiEqStatus,
  ChannelEq,
  ChannelRole,
  CompareSide,
  EqActor,
  EqAudit,
  EqChange,
  EqChannelStatus,
  EqIdea,
  EqLogEntry,
  EqProposal,
  FeedbackEvent,
  IdeaState,
  ManagedChannel,
  Notch,
  RingOutStatus,
  Role,
  SoundcheckState,
} from "./types";

export const SOUNDCHECK_SECS = 20;
/** A mic that hears nothing for this long during soundcheck is "No sound yet". */
const NO_SOUND_SECS = 8;
const RING_OUT_SECS = 4;
const TICK_MS = 500;
/** The first feedback event in the demo, after this long with AI EQ on. */
const AUTO_FEEDBACK_MS = 40_000;

export const DEFAULT_AIEQ_CONFIG: AiEqConfig = { enabled: true, toneKeeping: true, tap: "beforeEq" };

export interface DemoAiEqHooks {
  name(channel: number): string;
  /** The channels picked for AI (auto-mix's list). */
  picked(): ManagedChannel[];
  consoleOnline(): boolean;
  /** Recording or auto-mix running. */
  inService(): boolean;
  /** The plan includes AI EQ. */
  allowed(): boolean;
  role(): Role;
  fader(channel: number): number | null;
  setFader(channel: number, db: number): void;
  /** An EQ change on the demo desk, for the console event stream. */
  deskEq(channel: number, change: EqChange): void;
  status(s: AiEqStatus): void;
  log(e: EqLogEntry): void;
  /** When a recorded service started and how long it ran, for its report. */
  recording(id: string): Promise<{ startedAt: number; durationMs: number; title: string } | null>;
}

interface Recipe {
  title: string;
  reason: string;
  change(eq: ChannelEq): void;
  /** Where the mic hears the problem, for the spectrum. */
  bumps: number[];
}

type BaseMode = "notChecked" | "set" | "yours" | "undone";

interface Track {
  role: ChannelRole;
  eq: ChannelEq;
  baseline: ChannelEq | null;
  /** The EQ, mode and suggestion before the last soundcheck apply, for Before | After and Undo. */
  before: ChannelEq | null;
  beforeBase: BaseMode;
  applied: EqProposal | null;
  proposal: EqProposal | null;
  base: BaseMode;
  notch: Notch | null;
  tone: number;
  heard: number;
  waited: number;
  sc: SoundcheckState;
  changes: number;
  bumps: number[];
  ringUntil: number;
}

const bell = (freqHz: number, gainDb = 0, width = 1): ChannelEq["bands"][number] => ({
  kind: "bell",
  freqHz,
  width,
  gainDb,
});

/** What a church that already set its desk up might have on each kind of channel. */
export function startingEq(role: ChannelRole, name: string): ChannelEq {
  const free = bell(1000);
  switch (role) {
    case "speech":
      return name.startsWith("Pastor")
        ? {
            hpf: { on: true, freqHz: 120 },
            bands: [{ kind: "lowShelf", freqHz: 120, width: 1, gainDb: -1 }, bell(250, -2.5, 1.2), bell(4000, 1.5, 1.4), free],
          }
        : { hpf: { on: true, freqHz: 120 }, bands: [{ kind: "lowShelf", freqHz: 120, width: 1, gainDb: 0 }, bell(300, -1.5, 1.2), bell(3500, 1, 1.4), free] };
    case "leadVocal":
      return { hpf: { on: true, freqHz: 100 }, bands: [{ kind: "lowShelf", freqHz: 80, width: 1, gainDb: 0 }, bell(320, 0, 1), bell(3200, 0, 1.39), free] };
    case "backingVocal":
    case "choir":
      return { hpf: { on: true, freqHz: 100 }, bands: [{ kind: "lowShelf", freqHz: 100, width: 1, gainDb: 0 }, bell(350, 0, 1.2), bell(3000, 0, 1.4), free] };
    case "bass":
      return { hpf: { on: false, freqHz: 30 }, bands: [{ kind: "lowShelf", freqHz: 80, width: 1, gainDb: 1 }, bell(250, 0, 1), bell(800, 0, 1), free] };
    case "acousticGuitar":
      return { hpf: { on: true, freqHz: 60 }, bands: [{ kind: "lowShelf", freqHz: 100, width: 1, gainDb: 0 }, bell(250, 0, 1), bell(3000, 0, 1.2), free] };
    default:
      return flatEq();
  }
}

/** What the soundcheck proposes for a channel, or null when it sounds good. Scripted for the demo. */
function recipeFor(role: ChannelRole, name: string, nth: number): Recipe | null {
  switch (role) {
    case "speech":
      return nth === 0
        ? {
            title: "Less boom from the lav",
            reason: "The lav sits close to the chest, so there's extra boom around 250 Hz.",
            change: (eq) => {
              eq.bands[1].gainDb = -4;
            },
            bumps: [250],
          }
        : null;
    case "leadVocal":
      return {
        title: "Less boxy, less harsh on loud notes",
        // The desk's own low cut stays as the person set it (the Rust guardrails do the same).
        reason: "There's a buildup around 320 Hz and an edge near 3.2 kHz.",
        change: (eq) => {
          eq.bands[1].gainDb = -3;
          eq.bands[2].gainDb = -2;
        },
        bumps: [320, 3200],
      };
    case "backingVocal":
    case "choir":
      return nth === 0
        ? {
            title: "Less nasal",
            reason: "There's a honk near 1 kHz on the louder notes, and some handling rumble.",
            change: (eq) => {
              eq.hpf = { on: true, freqHz: 120 };
              eq.bands[1].freqHz = 1000;
              eq.bands[1].gainDb = -2;
            },
            bumps: [1000],
          }
        : {
            title: "Less muddy",
            reason: `A buildup around 350 Hz is clouding the words on ${name}.`,
            change: (eq) => {
              eq.bands[1].gainDb = -2.5;
            },
            bumps: [350],
          };
    case "bass":
      return {
        title: "Tighter low end",
        reason: "Some mud around 250 Hz and rumble under the lowest notes.",
        change: (eq) => {
          eq.hpf = { on: true, freqHz: 30 };
          eq.bands[1].gainDb = -2;
        },
        bumps: [250],
      };
    default:
      return {
        title: "Less boxy",
        reason: "There's a buildup around 400 Hz.",
        change: (eq) => {
          eq.bands[1].freqHz = 400;
          eq.bands[1].gainDb = -2;
        },
        bumps: [400],
      };
  }
}

/** Where each channel rings during the feedback check (null: it didn't). */
function ringFor(name: string, role: ChannelRole): { hz: number | null; ceilingDb: number | null } {
  if (name.startsWith("Pastor")) return { hz: 2500, ceilingDb: -2 };
  if (role === "speech") return { hz: 3150, ceilingDb: 1.5 };
  if (role === "leadVocal") return { hz: 4000, ceilingDb: -1 };
  if (name.endsWith("1")) return { hz: null, ceilingDb: null };
  return { hz: 630, ceilingDb: -4 };
}

/** A believable long-term spectrum, dB around its average. */
function spectrum(role: ChannelRole, bumps: number[], ringHz: number | null, jitter: () => number): number[] {
  const voice = role === "speech" || role === "leadVocal" || role === "backingVocal" || role === "choir";
  return SPECTRUM_FREQS.map((f) => {
    const lf = Math.log10(f);
    let v: number;
    if (role === "bass" || role === "kick") v = 4 - 22 * Math.max(0, lf - 2.6) - 10 * Math.max(0, 1.6 - lf);
    else if (voice) v = 2 - 30 * Math.max(0, 2.15 - lf) - 14 * Math.max(0, lf - 3.5);
    else v = 0 - 18 * Math.max(0, 1.9 - lf) - 10 * Math.max(0, lf - 3.6);
    for (const b of bumps) v += 6 * Math.exp(-(((lf - Math.log10(b)) / 0.1) ** 2));
    if (ringHz) v += 18 * Math.exp(-(((lf - Math.log10(ringHz)) / 0.03) ** 2));
    return Math.round((v + jitter()) * 10) / 10;
  });
}

const AI: EqActor = { kind: "ai", role: null, name: null, userId: null, where: null };

export function createDemoAiEq(hooks: DemoAiEqHooks) {
  let config: AiEqConfig = { ...DEFAULT_AIEQ_CONFIG };
  let frozen = false;
  const tracks = new Map<number, Track>();
  const log: EqLogEntry[] = [];
  let soundcheck: { running: boolean; order: number[] } | null = null;
  let ringOut: (RingOutStatus & { order: number[]; secs: number }) | null = null;
  let compare: { channel: number; side: CompareSide } | null = null;
  let feedback: FeedbackEvent | null = null;
  let feedbackCount = 0;
  let feedbackId = 0;
  let autoFeedbackAt: number | null = null;
  let profileDiffers: number[] | null = null;
  const profiles = new Map<number, ChannelEq>();
  const ideas = new Map<string, EqIdea[]>();
  let toneClock = 0;
  let seed = 7;
  const jitter = () => {
    seed = (seed * 16807) % 2147483647;
    return (seed / 2147483647 - 0.5) * 1.6;
  };

  const person = (where: "app" | "desk" = "app"): EqActor => ({
    kind: "person",
    role: hooks.role(),
    name: hooks.role() === "volunteer" ? "Volunteer" : "Demo engineer",
    userId: "demo-user",
    where,
  });
  const engineer = () => hooks.role() !== "volunteer";
  const picked = () => hooks.picked().slice().sort((a, b) => a.channel - b.channel);
  const isPicked = (ch: number) => hooks.picked().some((c) => c.channel === ch);

  /** The channel's track; picked channels get one with the desk's starting EQ. */
  function track(ch: number): Track {
    let t = tracks.get(ch);
    const role = hooks.picked().find((c) => c.channel === ch)?.role ?? "other";
    if (!t) {
      const eq = startingEq(role, hooks.name(ch));
      // Last week's soundcheck covered most channels; the lead vocal and acoustic are new.
      const checked = role !== "leadVocal" && role !== "acousticGuitar";
      t = {
        role,
        eq,
        baseline: checked ? cloneEq(eq) : null,
        before: null,
        beforeBase: "notChecked",
        applied: null,
        proposal: null,
        base: checked ? "set" : "notChecked",
        notch: null,
        tone: 0,
        heard: 0,
        waited: 0,
        sc: "upNext",
        changes: 0,
        bumps: [],
        ringUntil: 0,
      };
      tracks.set(ch, t);
    }
    t.role = role;
    return t;
  }

  /** The desk's EQ for any input, for the console's snapshot on connect. */
  function deskEq(ch: number): ChannelEq {
    return isPicked(ch) || tracks.has(ch) ? track(ch).eq : startingEq("other", hooks.name(ch));
  }

  function setDesk(ch: number, eq: ChannelEq) {
    const t = track(ch);
    const changes = diffEq(t.eq, eq);
    t.eq = cloneEq(eq);
    for (const c of changes) hooks.deskEq(ch, c);
  }

  function record(entry: Omit<EqLogEntry, "atMs" | "channelName">) {
    const e: EqLogEntry = { ...entry, atMs: Date.now(), channelName: hooks.name(entry.channel) };
    log.unshift(e);
    log.length = Math.min(log.length, 500);
    hooks.log(e);
  }

  function ensureProfiles() {
    if (profileDiffers !== null) return;
    // Someone loaded an older scene on the backing vocals: the desk differs from last Sunday's profile.
    profileDiffers = [];
    for (const c of picked()) {
      const t = track(c.channel);
      profiles.set(c.channel, cloneEq(t.eq));
      if (c.role === "backingVocal") {
        const desk = cloneEq(t.eq);
        desk.bands[2].gainDb = 2;
        t.eq = desk;
        t.baseline = cloneEq(desk);
        profileDiffers.push(c.channel);
      }
    }
  }

  const enabled = () => config.enabled && hooks.allowed();
  const live = () => enabled() && !frozen && hooks.consoleOnline();

  function mode(t: Track): EqChannelStatus["mode"] {
    if (!enabled()) return "off";
    if (!hooks.consoleOnline()) return "consoleOffline";
    if (t.base === "yours" || t.base === "undone") return t.base;
    if (frozen) return "frozen";
    if (t.base === "notChecked") return "notChecked";
    if (t.role === "speech" && config.toneKeeping && hooks.inService()) return "keeping";
    if (t.notch) return "notch";
    return "set";
  }

  function status(): AiEqStatus {
    ensureProfiles();
    const now = Date.now();
    const order = soundcheck?.order ?? [];
    return {
      enabled: enabled(),
      frozen,
      consoleOnline: hooks.consoleOnline(),
      eqSupported: true,
      audioOk: true,
      toneKeeping: config.toneKeeping,
      inService: hooks.inService(),
      soundcheck: soundcheck && {
        running: soundcheck.running,
        channels: order.map((ch) => {
          const t = track(ch);
          return { channel: ch, state: t.sc, heardSecs: Math.round(t.heard * 10) / 10, changes: t.changes };
        }),
      },
      compare,
      ringOut: ringOut && { running: ringOut.running, channel: ringOut.channel, results: ringOut.results },
      feedback,
      profileDiffers: [...(profileDiffers ?? [])],
      channels: picked().map((c) => {
        const t = track(c.channel);
        const silent = !hasSound(c.channel);
        return {
          channel: c.channel,
          name: hooks.name(c.channel),
          role: c.role,
          mode: mode(t),
          eq: cloneEq(t.eq),
          baseline: t.baseline && cloneEq(t.baseline),
          proposal: t.proposal,
          notch: t.notch,
          toneOffsetDb: c.role === "speech" && t.base !== "notChecked" ? t.tone : null,
          spectrum: silent ? null : spectrum(c.role, t.bumps, t.ringUntil > now ? (t.notch?.hz ?? null) : null, jitter),
          heardSecs: Math.round(t.heard * 10) / 10,
          differsFromProfile: false,
        };
      }),
    };
  }

  const push = () => hooks.status(status());

  /** The demo's acoustic guitar is unplugged today, so soundcheck hears nothing on it. */
  const hasSound = (ch: number) => !/acous|ac gtr/i.test(hooks.name(ch));

  function finishListening(ch: number, t: Track) {
    const sameRole = picked().filter((c) => c.role === t.role && c.channel < ch).length;
    const recipe = recipeFor(t.role, hooks.name(ch), sameRole);
    if (!recipe) {
      t.sc = "soundsGood";
      t.proposal = null;
      t.changes = 0;
      return;
    }
    const eq = cloneEq(t.eq);
    recipe.change(eq);
    const changes = describeChanges(t.eq, eq);
    t.bumps = recipe.bumps;
    t.proposal = { title: recipe.title, reason: recipe.reason, eq, changes };
    t.changes = changes.length;
    t.sc = "done";
  }

  function tickSoundcheck(secs: number) {
    if (!soundcheck?.running) return;
    const order = soundcheck.order;
    let listening = order.filter((ch) => track(ch).sc === "listening");
    if (listening.length === 0) {
      const next = order.find((ch) => track(ch).sc === "upNext");
      if (next === undefined) {
        soundcheck.running = false;
        return;
      }
      track(next).sc = "listening";
      listening = [next];
    }
    // Two singers of the same kind can go at once, as on a real stage.
    const role = track(listening[0]).role;
    if (listening.length === 1 && role !== "speech") {
      const partner = order.find((ch) => track(ch).sc === "upNext" && track(ch).role === role);
      if (partner !== undefined) {
        track(partner).sc = "listening";
        listening.push(partner);
      }
    }
    for (const ch of listening) {
      const t = track(ch);
      t.waited += secs;
      if (hasSound(ch)) t.heard = Math.min(SOUNDCHECK_SECS, t.heard + secs);
      if (t.heard >= SOUNDCHECK_SECS) finishListening(ch, t);
      else if (!hasSound(ch) && t.waited >= NO_SOUND_SECS) t.sc = "noSound";
    }
  }

  function tickRingOut(secs: number) {
    if (!ringOut?.running || ringOut.channel === null) return;
    ringOut.secs += secs;
    if (ringOut.secs < RING_OUT_SECS) return;
    const ch = ringOut.channel;
    const r = ringFor(hooks.name(ch), track(ch).role);
    ringOut.results = [...ringOut.results, { channel: ch, ...r }];
    const next = ringOut.order[ringOut.order.indexOf(ch) + 1];
    ringOut.secs = 0;
    ringOut.channel = next ?? null;
    if (next === undefined) ringOut.running = false;
  }

  function tickTone(secs: number) {
    if (!live() || !config.toneKeeping || !hooks.inService()) return;
    toneClock += secs;
    if (toneClock < 10) return;
    toneClock = 0;
    for (const c of picked()) {
      if (c.role !== "speech") continue;
      const t = track(c.channel);
      if (t.base !== "set" || !t.baseline) continue;
      // The voice wanders a little against its profile; follow it 0.5 dB at a time, within ±3 dB.
      const want = Math.round(Math.sin(Date.now() / 45_000 + c.channel) * 2) / 2;
      if (want === t.tone) continue;
      const to = Math.max(-3, Math.min(3, t.tone + Math.sign(want - t.tone) * 0.5));
      const eq = cloneEq(t.eq);
      const from = eq.bands[1].gainDb;
      const boosted = to > t.tone;
      eq.bands[1].gainDb = t.baseline.bands[1].gainDb + to;
      t.tone = to;
      setDesk(c.channel, eq);
      record({
        channel: c.channel,
        action: "tone",
        changes: [`${formatHz(eq.bands[1].freqHz)}  ${formatGain(from).replace(" dB", "")} → ${formatGain(eq.bands[1].gainDb)}`],
        reason: `${hooks.name(c.channel)} sounded a little ${boosted ? "thin" : "boomy"} against their profile.`,
        by: AI,
        appliedBy: null,
      });
    }
  }

  function tick(secs = TICK_MS / 1000) {
    tickSoundcheck(secs);
    tickRingOut(secs);
    tickTone(secs);
    if (live() && autoFeedbackAt === null) autoFeedbackAt = Date.now() + AUTO_FEEDBACK_MS;
    if (autoFeedbackAt !== null && Date.now() >= autoFeedbackAt && feedbackCount === 0 && live()) {
      const speech = picked().find((c) => c.role === "speech");
      if (speech) triggerFeedback(speech.channel, 2500);
    }
    push();
  }

  /** A mic starts ringing: notch it on band 4 (or pull the fader only when band 4 is in use). */
  function triggerFeedback(ch: number, hz: number) {
    if (!live() || !isPicked(ch)) return;
    const t = track(ch);
    const eq = cloneEq(t.eq);
    const band = eq.bands[RESERVED_BAND];
    const busy = t.notch !== null && Math.abs(t.notch.hz - hz) > hz * 0.05;
    const notchDb = busy ? null : -6;
    const changes: string[] = [];
    if (notchDb !== null) {
      changes.push(`${formatHz(hz)}  ${formatGain(band.gainDb).replace(" dB", "")} → ${formatGain(notchDb)}`);
      eq.bands[RESERVED_BAND] = { kind: "bell", freqHz: hz, width: 1 / 9, gainDb: notchDb };
      t.notch = { hz, gainDb: notchDb, atMs: Date.now() };
      setDesk(ch, eq);
    }
    const fader = hooks.fader(ch);
    const cut = 3;
    if (fader !== null) {
      hooks.setFader(ch, Math.round((fader - (busy ? 6 : cut)) * 2) / 2);
      changes.push(`Fader  ${formatGain(fader).replace(" dB", "")} → ${formatGain(fader - (busy ? 6 : cut))}`);
    }
    t.ringUntil = Date.now() + 10_000;
    feedbackCount += 1;
    feedbackId += 1;
    feedback = {
      id: feedbackId,
      atMs: Date.now(),
      channel: ch,
      channelName: hooks.name(ch),
      hz,
      notchDb,
      faderCutDb: busy ? 6 : cut,
      countToday: feedbackCount,
    };
    record({
      channel: ch,
      action: "feedback",
      changes,
      reason: busy
        ? `${hooks.name(ch)} rang at ${formatHz(hz)}. Band 4 is already used, so only the fader came down.`
        : `${hooks.name(ch)} rang at ${formatHz(hz)}.`,
      by: AI,
      appliedBy: null,
    });
    push();
  }

  function apply(ch: number) {
    if (hooks.inService()) throw "Soundcheck changes wait until the service ends.";
    const t = track(ch);
    if (!t.proposal) return;
    const p = t.proposal;
    t.before = cloneEq(t.eq);
    t.beforeBase = t.base;
    t.applied = p;
    setDesk(ch, p.eq);
    t.baseline = cloneEq(p.eq);
    t.base = "set";
    t.tone = 0;
    t.proposal = null;
    t.sc = "applied";
    record({ channel: ch, action: "soundcheck", changes: p.changes, reason: p.reason, by: AI, appliedBy: person() });
  }

  function undo(ch: number) {
    const t = track(ch);
    if (!hooks.inService() && t.before && t.sc === "applied") {
      // Straight after a soundcheck apply: back to what was there, with the suggestion offered again.
      const from = t.eq;
      setDesk(ch, t.before);
      t.proposal = t.applied;
      t.sc = "done";
      t.base = t.beforeBase;
      t.baseline = t.beforeBase === "notChecked" ? null : cloneEq(t.before);
      t.before = null;
      t.applied = null;
      if (compare?.channel === ch) compare = null;
      record({ channel: ch, action: "undo", changes: describeChanges(from, t.eq), reason: "Back to the EQ before the soundcheck change.", by: person(), appliedBy: null });
      return;
    }
    const target = cloneEq(t.baseline ?? t.eq);
    target.bands[RESERVED_BAND] = { ...target.bands[RESERVED_BAND], gainDb: 0 };
    const changes = describeChanges(t.eq, target);
    setDesk(ch, target);
    t.notch = null;
    t.tone = 0;
    t.base = "undone";
    if (feedback?.channel === ch) feedback = null;
    record({ channel: ch, action: "undo", changes, reason: "Back to its soundcheck EQ. AI EQ holds it until you hand it back.", by: person(), appliedBy: null });
  }

  setInterval(() => tick(), TICK_MS);

  return {
    getConfig: () => ({ ...config }),
    setConfig(next: AiEqConfig): AiEqConfig {
      if (next.enabled && !hooks.allowed()) throw "AI EQ is in Pro. See Settings.";
      if ((next.toneKeeping !== config.toneKeeping || next.tap !== config.tap) && !engineer())
        throw "Ask an engineer or admin to change how AI EQ is set up.";
      config = { ...next };
      push();
      return { ...config };
    },
    status,
    log: (limit = 200) => log.slice(0, limit),
    deskEq,
    soundcheckStart() {
      if (!hooks.allowed()) throw "AI EQ is in Pro. See Settings.";
      if (hooks.inService()) throw "Soundcheck waits until the service ends. Stop recording and auto-mix first.";
      const order = picked()
        .slice()
        .sort((a, b) => roleGroup(a.role) - roleGroup(b.role) || a.channel - b.channel)
        .map((c) => c.channel);
      for (const ch of order) {
        const t = track(ch);
        Object.assign(t, { heard: 0, waited: 0, sc: "upNext", changes: 0, proposal: null });
      }
      soundcheck = { running: true, order };
      tick(0);
    },
    soundcheckStop() {
      if (!soundcheck) return;
      soundcheck.running = false;
      for (const ch of soundcheck.order) {
        const t = track(ch);
        if (t.sc === "listening") t.sc = t.heard > 0 ? "upNext" : "noSound";
      }
      push();
    },
    apply(ch: number) {
      apply(ch);
      push();
    },
    applyAll() {
      if (hooks.inService()) throw "Soundcheck changes wait until the service ends.";
      for (const c of picked()) if (track(c.channel).proposal) apply(c.channel);
      push();
    },
    skip(ch: number) {
      const t = track(ch);
      t.proposal = null;
      t.sc = "skipped";
      push();
    },
    keepMyEq() {
      if (!engineer()) throw "Ask an engineer or admin to keep the console's EQ.";
      for (const c of picked()) {
        const t = track(c.channel);
        t.baseline = cloneEq(t.eq);
        t.proposal = null;
        t.base = "set";
        if (soundcheck && t.sc !== "applied") t.sc = "skipped";
      }
      if (soundcheck) soundcheck.running = false;
      push();
    },
    undo(ch: number) {
      undo(ch);
      push();
    },
    undoAll() {
      for (const c of picked()) {
        const t = track(c.channel);
        if (t.notch || t.tone !== 0 || t.base === "yours") undo(c.channel);
      }
      push();
    },
    handBack(ch: number) {
      const t = track(ch);
      if (t.base !== "yours" && t.base !== "undone") return;
      t.base = "set";
      t.baseline = cloneEq(t.eq);
      t.tone = 0;
      record({ channel: ch, action: "handBack", changes: [], reason: "Handed back to AI EQ from where you left it.", by: person(), appliedBy: null });
      push();
    },
    setEq(ch: number, eq: ChannelEq) {
      const t = track(ch);
      const changes = describeChanges(t.eq, eq);
      if (changes.length === 0) return;
      setDesk(ch, eq);
      if (compare?.channel === ch) compare = null;
      if (isPicked(ch) && t.base !== "yours") {
        t.base = "yours";
        record({ channel: ch, action: "person", changes, reason: null, by: person(), appliedBy: null });
      }
      push();
    },
    compare(ch: number, side: CompareSide | null) {
      if (hooks.inService()) throw "Before and After wait until the service ends.";
      const t = track(ch);
      if (!t.before || !t.baseline) throw "Apply a soundcheck change first, then compare.";
      setDesk(ch, side === "before" ? t.before : t.baseline);
      compare = side ? { channel: ch, side } : null;
      push();
    },
    restoreProfile() {
      for (const ch of profileDiffers ?? []) {
        const p = profiles.get(ch);
        if (!p) continue;
        const t = track(ch);
        record({ channel: ch, action: "soundcheck", changes: describeChanges(t.eq, p), reason: "Restored last Sunday's EQ.", by: AI, appliedBy: person() });
        setDesk(ch, p);
        t.baseline = cloneEq(p);
      }
      profileDiffers = [];
      push();
    },
    dismissProfile() {
      profileDiffers = [];
      push();
    },
    ringOutStart() {
      if (!engineer()) throw "Only engineers and admins can run the feedback check.";
      if (hooks.inService()) throw "The feedback check waits until the service ends.";
      const order = picked()
        .filter((c) => roleGroup(c.role) <= 1)
        .sort((a, b) => roleGroup(a.role) - roleGroup(b.role) || a.channel - b.channel)
        .map((c) => c.channel);
      if (order.length === 0) throw "Pick a speech or vocal mic first.";
      ringOut = { running: true, channel: order[0], results: [], order, secs: 0 };
      push();
    },
    ringOutStop() {
      if (ringOut) ringOut = { ...ringOut, running: false, channel: null };
      push();
    },
    dismissFeedback() {
      feedback = null;
      push();
    },
    freeze() {
      frozen = true;
      push();
    },
    resume() {
      frozen = false;
      push();
    },
    triggerFeedback,
    /** Runs the demo clock forward (for screenshots and tests). */
    fastForward(secs: number) {
      for (let i = 0; i < Math.ceil(secs / 0.5); i++) {
        tickSoundcheck(0.5);
        tickRingOut(0.5);
      }
      push();
    },
    async ideas(recordingId: string | null): Promise<EqIdea[]> {
      if (recordingId === null) return [...ideas.values()].flat().filter((i) => i.state === "waiting");
      return ideasFor(recordingId);
    },
    setIdea(id: string, state: IdeaState): EqIdea[] {
      if (!engineer()) throw "Ask an engineer or admin to keep or dismiss ideas.";
      for (const list of ideas.values()) {
        const idea = list.find((i) => i.id === id);
        if (idea) {
          idea.state = state;
          return list.map((i) => ({ ...i }));
        }
      }
      throw "That idea is gone. Reopen the service.";
    },
    async audit(recordingId: string): Promise<EqAudit> {
      const rec = await hooks.recording(recordingId);
      if (!rec) return { entries: [], ideas: [] };
      const at = (frac: number) => Math.round(rec.durationMs * frac);
      const mk = (frac: number, channel: number, action: EqLogEntry["action"], changes: string[], reason: string | null, by: EqActor) => ({
        atMs: rec.startedAt + at(frac),
        tMs: at(frac),
        channel,
        channelName: hooks.name(channel),
        action,
        changes,
        reason,
        by,
        appliedBy: null,
      });
      const speech = picked().filter((c) => c.role === "speech").map((c) => c.channel);
      const pastor = speech[0] ?? 0;
      const bgv = picked().find((c) => c.role === "backingVocal")?.channel ?? 2;
      const desk: EqActor = { kind: "person", role: "engineer", name: "Sam", userId: null, where: "desk" };
      const entries = [
        mk(0.69, pastor, "feedback", [`2.5 kHz  0.0 → ${formatGain(-6)}`, `Fader  ${MINUS}5.0 → ${formatGain(-8)}`], "Pastor rang at 2.5 kHz.", AI),
        mk(0.75, pastor, "tone", [`250 Hz  ${MINUS}3.0 → ${formatGain(-2.5)}`], "Pastor sounded a little thin against their profile.", AI),
        mk(0.82, pastor, "tone", [`250 Hz  ${MINUS}2.5 → ${formatGain(-2)}`], "Kept Pastor's voice close to their profile over the message.", AI),
        mk(0.97, bgv, "person", [`350 Hz  ${MINUS}2.0 → 0.0 dB`], null, desk),
      ];
      if (speech[1] !== undefined)
        entries.push(mk(0.2, speech[1], "tone", [`3.5 kHz  +1.0 → ${formatGain(1.5)}`], "Brightened the host a touch during announcements.", AI));
      entries.sort((a, b) => a.tMs - b.tMs);
      return { entries, ideas: await ideasFor(recordingId) };
    },
  };

  async function ideasFor(recordingId: string): Promise<EqIdea[]> {
    let list = ideas.get(recordingId);
    if (!list) {
      const rec = await hooks.recording(recordingId);
      if (!rec) return [];
      const time = new Date(rec.startedAt + rec.durationMs * 0.69).toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" }).toLowerCase();
      const byRole = (r: ChannelRole) => picked().find((c) => c.role === r)?.channel;
      const pastor = byRole("speech") ?? 0;
      const bgv = byRole("backingVocal") ?? 2;
      const acoustic = byRole("acousticGuitar") ?? 13;
      const mk = (n: number, channel: number, title: string, change: string, reason: string): EqIdea => ({
        id: `${recordingId}-eq-${n}`,
        recordingId,
        atMs: rec.startedAt + rec.durationMs,
        channel,
        channelName: hooks.name(channel),
        title,
        change,
        reason,
        state: "waiting",
      });
      list = [mk(1, pastor, `Keep the ${hooks.name(pastor)} notch`, `2.5 kHz ${formatGain(-6)}`, `rang once at ${time}`)];
      if (!/wednesday/i.test(rec.title)) {
        list.push(
          mk(2, bgv, `Warm up ${hooks.name(bgv)}`, `350 Hz ${MINUS}2.0 → ${formatGain(-1)}`, "sounded thin all service"),
          mk(3, acoustic, `Less rumble on ${hooks.name(acoustic)}`, "Low cut 60 → 100 Hz", "stage noise under the guitar"),
        );
      }
      ideas.set(recordingId, list);
    }
    return list.map((i) => ({ ...i }));
  }
}

export type DemoAiEq = ReturnType<typeof createDemoAiEq>;
