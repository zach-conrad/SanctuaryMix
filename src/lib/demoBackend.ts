import type { Backend } from "./backend";
import { eqSnapshot } from "./aieq";
import { createDemoAiEq } from "./demoAiEq";
import { createDemoAuth } from "./demoAuth";
import { createDemoAutomix, guessRole } from "./demoAutomix";
import { createDemoRecorder } from "./demoRecorder";
import { createDemoSpl } from "./demoSpl";
import type {
  Adjustment,
  AiEqStatus,
  AutoMixStatus,
  ChannelRole,
  ConsoleEvent,
  EqLogEntry,
  MeterFrame,
  Session,
  Sound,
} from "./types";

const NAMES = [
  "Pastor", "Worship Ld", "BGV 1", "BGV 2", "Kick", "Snare", "Hat", "Tom 1", "Tom 2",
  "OH L", "OH R", "Bass DI", "Elec Gtr", "Acous Gtr", "Keys L", "Keys R", "Pad L", "Pad R",
  "Choir L", "Choir R", "Handheld 1", "Handheld 2", "Video L", "Video R", "Playback L",
  "Playback R", "Lapel 1", "Lapel 2", "Ambient L", "Ambient R", "Spare 1", "Spare 2",
];

const SOUND_OF_ROLE: Record<ChannelRole, Sound | null> = {
  speech: "speech", leadVocal: "singing", backingVocal: "singing", choir: "choir", kick: "drums", bass: "bass",
  drums: "drums", keysPads: "keys", pianoOrgan: "piano", electricGuitar: "electricGuitar",
  acousticGuitar: "acousticGuitar", playback: "music", other: "other",
};

/** What the pretend listening models hear. Two channels disagree with their names on purpose. */
function demoSound(ch: number): Sound | null {
  if (ch >= 30) return null;
  if (ch === 21) return "acousticGuitar"; // "Handheld 2" is on a guitar amp today
  if (ch === 26) return "music"; // "Lapel 1": nobody is wearing it, it only hears the band
  return SOUND_OF_ROLE[guessRole(NAMES[ch] ?? "")];
}

/** Picked for AI when the demo opens, so auto-mix and AI EQ have channels to show. */
const DEMO_PICKS = [0, 1, 2, 3, 11, 13, 20];

/** Rough "typical level" per channel so the demo looks like a real service. */
function baseLevel(ch: number): number {
  if (ch >= 30) return -120; // spares are silent
  if (ch === 4 || ch === 5) return -10; // kick/snare run hot
  if (ch === 1) return -14;
  return -22 - (ch % 5) * 3;
}

export function createDemoBackend(): Backend {
  const meterListeners = new Set<(f: MeterFrame) => void>();
  const consoleListeners = new Set<(e: ConsoleEvent) => void>();
  const automixListeners = new Set<(s: AutoMixStatus) => void>();
  const adjustmentListeners = new Set<(a: Adjustment) => void>();
  const emit = (e: ConsoleEvent) => consoleListeners.forEach((cb) => cb(e));
  // What the demo desk holds. Faders start at 0 dB, like the UI assumes.
  const faders = new Map<number, number | null>();
  const mutes = new Map<number, boolean>();
  let connected = false;
  const auth = createDemoAuth();
  const sessionListeners = new Set<(session: Session) => void>();
  const moveFader = (index: number, db: number | null) => {
    faders.set(index, db);
    emit({ type: "fader", id: { kind: "input", index }, db });
    automix.onFader(index, db);
  };
  const automix = createDemoAutomix({
    fader: (ch) => (connected ? (faders.get(ch) ?? null) : null),
    name: (ch) => NAMES[ch] ?? `Ch ${ch + 1}`,
    muted: (ch) => mutes.get(ch) ?? false,
    hear: (ch) => {
      const sound = demoSound(ch);
      return { sound, voice: sound === "speech" || sound === "singing" || sound === "choir" };
    },
    setFader: (ch, db) => {
      moveFader(ch, db);
      recorder.noteChange("assist", { kind: "input", index: ch }, { db });
    },
    status: (s) => automixListeners.forEach((cb) => cb(s)),
    adjustment: (a) => adjustmentListeners.forEach((cb) => cb(a)),
  });
  // Service recordings, playback and replay (docs/RECORDINGS.md).
  const recorder = createDemoRecorder({
    channelCount: () => (connected ? faders.size : 0),
    name: (ch) => NAMES[ch] ?? `Ch ${ch + 1}`,
    fader: (ch) => faders.get(ch) ?? 0,
    muted: (ch) => mutes.get(ch) ?? false,
    setFader: (ch, db) => moveFader(ch, db),
    setMute: (ch, muted) => {
      mutes.set(ch, muted);
      emit({ type: "mute", id: { kind: "input", index: ch }, muted });
    },
    automixOff: () => {
      const on = automix.status().engaged;
      if (on) automix.engage(false);
      return on;
    },
  });
  automix.setConfig({
    ...automix.getConfig(),
    channels: DEMO_PICKS.map((channel) => ({ channel, role: guessRole(NAMES[channel]) })),
  });
  // AI EQ: soundcheck proposals, feedback notches and speech tone keeping (docs/AIEQ.md).
  const aieqListeners = new Set<(s: AiEqStatus) => void>();
  const aieqLogListeners = new Set<(e: EqLogEntry) => void>();
  const aieq = createDemoAiEq({
    name: (ch) => NAMES[ch] ?? `Ch ${ch + 1}`,
    picked: () => automix.getConfig().channels,
    consoleOnline: () => connected,
    inService: () => automix.status().engaged || recording,
    allowed: () => auth.session().access.entitlements.aiEq,
    role: () => auth.session().role,
    fader: (ch) => (connected ? (faders.get(ch) ?? null) : null),
    // The feedback guard's fader cut is an AI move, not a person's, so auto-mix doesn't let go of the channel.
    setFader: (ch, db) => {
      faders.set(ch, db);
      emit({ type: "fader", id: { kind: "input", index: ch }, db });
      recorder.noteChange("assist", { kind: "input", index: ch }, { db });
    },
    deskEq: (ch, change) => {
      if (connected) emit({ type: "eq", id: { kind: "input", index: ch }, change });
    },
    status: (s) => aieqListeners.forEach((cb) => cb(s)),
    log: (e) => aieqLogListeners.forEach((cb) => cb(e)),
    recording: async (id) => {
      try {
        const r = await recorder.getRecording(id);
        return { startedAt: r.startedAt, durationMs: r.durationMs, title: r.title };
      } catch {
        return null;
      }
    },
  });
  let recording = false;
  void recorder.onRecorder((s) => {
    recording = s.active !== null;
  });
  // Handy in `npm run dev`: trigger feedback or skip ahead from the browser console.
  (window as unknown as Record<string, unknown>).sanctuaryDemo = {
    feedback: (channel = 0, hz = 2500) => aieq.triggerFeedback(channel, hz),
    fastForward: (seconds = 20) => aieq.fastForward(seconds),
  };
  const spl = createDemoSpl();
  let timer: ReturnType<typeof setInterval> | null = null;
  let channels = 0;
  const drift: number[] = [];

  return {
    isDemo: true,
    async microphoneAccess() {
      return "granted";
    },
    async requestMicrophoneAccess() {
      return "granted";
    },
    async openMicrophoneSettings() {},
    async listAudioDevices() {
      return [
        { name: "Dante Virtual Soundcard", maxInputChannels: 64, defaultSampleRate: 48000, isDefault: false, isDante: true },
        { name: "MacBook Pro Microphone", maxInputChannels: 1, defaultSampleRate: 48000, isDefault: true, isDante: false },
      ];
    },
    async startMetering(device) {
      channels = device?.includes("Dante") || device === null ? 32 : 1;
      if (timer) clearInterval(timer);
      timer = setInterval(() => {
        const t = performance.now() / 1000;
        const frame: MeterFrame = {
          sampleRate: 48000,
          channels: Array.from({ length: channels }, (_, ch) => {
            drift[ch] = Math.max(-6, Math.min(6, (drift[ch] ?? 0) + (Math.random() - 0.5) * 1.5));
            const base = baseLevel(ch);
            if (base <= -120) return { channel: ch, peakDb: -120, rmsDb: -120, clipped: false };
            const phrase = Math.sin(t * 0.7 + ch) > -0.6 ? 0 : -25; // singers breathe
            const rms = base + drift[ch] + phrase;
            const peak = rms + 6 + Math.random() * 4;
            return { channel: ch, peakDb: peak, rmsDb: rms, clipped: peak > -0.1 };
          }),
        };
        automix.onMeters(frame);
        meterListeners.forEach((cb) => cb(frame));
      }, 33);
      spl.start();
      return { deviceName: device ?? "Dante Virtual Soundcard", channels, sampleRate: 48000 };
    },
    async stopMetering() {
      if (timer) clearInterval(timer);
      timer = null;
      spl.stop();
    },
    async connectConsole(config) {
      await new Promise((r) => setTimeout(r, 400));
      connected = true;
      emit({ type: "connected", model: config.model === "dlive" ? "Allen & Heath dLive (demo)" : "Practice console" });
      for (let i = 0; i < config.inputCount; i++) {
        emit({ type: "name", id: { kind: "input", index: i }, name: NAMES[i] ?? `Ch ${i + 1}` });
        // A desk someone already set up: most faders a little under unity.
        if (!faders.has(i)) faders.set(i, i < 30 ? -5 : 0);
        emit({ type: "fader", id: { kind: "input", index: i }, db: faders.get(i)! });
        for (const change of eqSnapshot(aieq.deskEq(i))) emit({ type: "eq", id: { kind: "input", index: i }, change });
      }
    },
    async disconnectConsole() {
      connected = false;
      emit({ type: "disconnected", reason: null });
    },
    async setFader(id, db) {
      recorder.noteChange("operator", id, { db });
      if (id.kind === "input") moveFader(id.index, db);
      else emit({ type: "fader", id, db });
    },
    async setMute(id, muted) {
      recorder.noteChange("operator", id, { muted });
      if (id.kind === "input") mutes.set(id.index, muted);
      emit({ type: "mute", id, muted });
    },
    async getSession() {
      return auth.session();
    },
    async beginSignIn() {
      // No website round trip in the browser demo: open the demo church straight away.
      const session = auth.signIn();
      sessionListeners.forEach((cb) => cb(session));
    },
    async onSession(cb) {
      sessionListeners.add(cb);
      return () => sessionListeners.delete(cb);
    },
    async onSessionError() {
      return () => {};
    },
    async signOut() {
      if (automix.status().engaged) throw "Turn off auto-mix before you sign out.";
      if ((await recorder.recorderStatus()).active) throw "Stop recording the service before you sign out.";
      return auth.signOut();
    },
    async openBilling() {
      window.open("https://sanctuarymix.vercel.app/account/", "_blank", "noopener");
    },
    async automixPresets() {
      return automix.presets();
    },
    async automixGuessRoles(names) {
      return names.map(guessRole);
    },
    async automixGetConfig() {
      return automix.getConfig();
    },
    async automixSetConfig(config) {
      return automix.setConfig(config);
    },
    async automixEngage(on) {
      const { entitlements } = auth.session().access;
      if (on && !entitlements.autoMix) throw "Auto-mix needs a SanctuaryMix plan. Sign in in Settings.";
      if (on && automix.getConfig().channels.length > entitlements.maxAiChannels)
        throw `Your plan covers up to ${entitlements.maxAiChannels} channels under auto-mix.`;
      automix.engage(on);
    },
    // One freeze for every AI move, levels and EQ, like the core.
    async automixFreeze() {
      automix.freeze();
      aieq.freeze();
    },
    async automixResume() {
      automix.resume();
      aieq.resume();
    },
    async automixResumeChannel(channel) {
      automix.resumeChannel(channel);
    },
    async automixUndo(channel) {
      automix.undo(channel);
    },
    async automixUndoAll() {
      automix.undoAll();
    },
    async automixStatus() {
      return automix.status();
    },
    async automixLog(limit) {
      return automix.log(limit);
    },
    async automixListenScan() {},
    async automixHeard(names) {
      return automix.heard(names);
    },
    async onAutomix(cb) {
      automixListeners.add(cb);
      return () => automixListeners.delete(cb);
    },
    async onAutomixAdjustment(cb) {
      adjustmentListeners.add(cb);
      return () => adjustmentListeners.delete(cb);
    },
    async aieqGetConfig() {
      return aieq.getConfig();
    },
    async aieqSetConfig(config) {
      return aieq.setConfig(config);
    },
    async aieqStatus() {
      return aieq.status();
    },
    async onAiEq(cb) {
      aieqListeners.add(cb);
      return () => aieqListeners.delete(cb);
    },
    async onAiEqLog(cb) {
      aieqLogListeners.add(cb);
      return () => aieqLogListeners.delete(cb);
    },
    async aieqLog(limit) {
      return aieq.log(limit);
    },
    async aieqSoundcheckStart() {
      aieq.soundcheckStart();
    },
    async aieqSoundcheckStop() {
      aieq.soundcheckStop();
    },
    async aieqApply(channel) {
      aieq.apply(channel);
    },
    async aieqApplyAll() {
      aieq.applyAll();
    },
    async aieqSkip(channel) {
      aieq.skip(channel);
    },
    async aieqKeepMyEq() {
      aieq.keepMyEq();
    },
    async aieqUndo(channel) {
      aieq.undo(channel);
    },
    async aieqUndoAll() {
      aieq.undoAll();
    },
    async aieqHandBack(channel) {
      aieq.handBack(channel);
    },
    async aieqSetEq(channel, eq) {
      aieq.setEq(channel, eq);
    },
    async aieqCompare(channel, side) {
      aieq.compare(channel, side);
    },
    async aieqRestoreProfile() {
      aieq.restoreProfile();
    },
    async aieqDismissProfile() {
      aieq.dismissProfile();
    },
    async aieqRingOutStart() {
      aieq.ringOutStart();
    },
    async aieqRingOutStop() {
      aieq.ringOutStop();
    },
    async aieqDismissFeedback() {
      aieq.dismissFeedback();
    },
    async aieqIdeas(recordingId) {
      return aieq.ideas(recordingId);
    },
    async aieqSetIdea(id, state) {
      return aieq.setIdea(id, state);
    },
    async aieqAudit(recordingId) {
      return aieq.audit(recordingId);
    },
    async onMeters(cb) {
      meterListeners.add(cb);
      return () => meterListeners.delete(cb);
    },
    async onConsole(cb) {
      consoleListeners.add(cb);
      return () => consoleListeners.delete(cb);
    },
    async splGetConfig() {
      return spl.getConfig();
    },
    async splSetConfig(config) {
      return spl.setConfig(config);
    },
    async splCalibrate(weighting, referenceDb) {
      spl.calibrate(weighting, referenceDb);
    },
    async splCancelCalibration() {
      spl.cancelCalibration();
    },
    async onSplCalibration(cb) {
      return spl.onCalibration(cb);
    },
    async splReset() {
      spl.reset();
    },
    async splReading() {
      return spl.reading();
    },
    async splHistory(seconds) {
      return spl.history(seconds);
    },
    async onSpl(cb) {
      return spl.listen(cb);
    },
    ...recorder,
    async startRecording(title) {
      if (!auth.session().access.entitlements.recordServices)
        throw "Recording services needs a SanctuaryMix plan. Sign in in Settings.";
      return recorder.startRecording(title);
    },
  };
}
