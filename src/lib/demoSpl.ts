// Pretend room loudness for the browser demo: a service that alternates
// speaking and worship sets, measured the way crates/audio-engine/src/spl.rs
// does it (energy averages, offset applied when read).

import type { CalibrationStatus, SplConfig, SplPoint, SplReading, Weighting } from "./types";

const DEFAULT_OFFSET_DB = 120;
const HISTORY_SECS = 2 * 60 * 60;
/** Ten minutes of pretend service already measured when the demo opens. */
const PREFILL_SECS = 10 * 60;
const TICK_MS = 100;

const toMs = (db: number) => 10 ** (db / 10);
const toDb = (ms: number) => 10 * Math.log10(Math.max(ms, 1e-14));

/** Target dBFS (A, C) for a moment in the service: talks, then a worship set. */
function targetDbfs(t: number): [number, number] {
  const cycle = t % 420;
  if (cycle < 120) {
    // Speaking: about 72 dBA at the default offset, little low end.
    const a = -48 + 3 * Math.sin(t * 1.3) * Math.sin(t * 0.21);
    return [a, a + 3];
  }
  // Music: builds through the song, kick and bass push C well above A.
  const swell = Math.sin(((cycle - 120) / 300) * Math.PI);
  const a = -36 + 8 * swell + 1.5 * Math.sin(t * 2.1);
  return [a, a + 7 + 2 * swell];
}

export interface DemoSpl {
  start(): void;
  stop(): void;
  getConfig(): SplConfig;
  setConfig(config: SplConfig): SplConfig;
  /** Pretends pink noise is playing: steady for five seconds, then done. */
  calibrate(weighting: Weighting, referenceDb: number): void;
  cancelCalibration(): void;
  onCalibration(cb: (s: CalibrationStatus) => void): () => void;
  reset(): void;
  reading(): SplReading | null;
  history(seconds: number): SplPoint[];
  listen(cb: (r: SplReading) => void): () => void;
}

export function createDemoSpl(): DemoSpl {
  // The demo room has an ambience mic on input 29 ("Ambient L").
  let config: SplConfig = { source: 28, offsetDb: DEFAULT_OFFSET_DB, calibrated: false, calibration: null };
  const listeners = new Set<(r: SplReading) => void>();
  const calListeners = new Set<(s: CalibrationStatus) => void>();
  let calTimer: ReturnType<typeof setInterval> | null = null;
  const emitCal = (s: CalibrationStatus) => calListeners.forEach((cb) => cb(s));
  const cancelCal = () => {
    if (calTimer) clearInterval(calTimer);
    calTimer = null;
  };
  let timer: ReturnType<typeof setInterval> | null = null;
  // Raw mean squares (dBFS terms), one per second.
  let points: { t: number; a: number; c: number }[] = [];
  let elapsed = 0;
  let total = { a: 0, c: 0 };
  let fast = { a: 0, c: 0 };
  let slow = { a: 0, c: 0 };
  let second = { a: 0, c: 0, n: 0 };
  let aMax = 0;
  let cPeak = 0;
  const serviceClock = () => elapsed + 900; // start partway into a service

  function step(dt: number) {
    const [ta, tc] = targetDbfs(serviceClock());
    const jitter = () => (Math.random() - 0.5) * 1.2;
    const a = toMs(ta + jitter());
    const c = toMs(tc + jitter());
    const kf = 1 - Math.exp(-dt / 0.125);
    const ks = 1 - Math.exp(-dt / 1);
    fast = { a: fast.a + (a - fast.a) * kf, c: fast.c + (c - fast.c) * kf };
    slow = { a: slow.a + (a - slow.a) * ks, c: slow.c + (c - slow.c) * ks };
    aMax = Math.max(aMax, fast.a);
    cPeak = Math.max(cPeak, c * 4); // crest factor about 6 dB
    total = { a: total.a + a * dt, c: total.c + c * dt };
    second = { a: second.a + a * dt, c: second.c + c * dt, n: second.n + dt };
    elapsed += dt;
    if (second.n >= 1 - 1e-9) {
      points.push({ t: elapsed, a: second.a / second.n, c: second.c / second.n });
      if (points.length > HISTORY_SECS) points.shift();
      second = { a: 0, c: 0, n: 0 };
    }
  }

  function leq(secs: number) {
    const recent = points.slice(-secs);
    const n = recent.length + second.n;
    if (n === 0) return { a: 0, c: 0 };
    const sum = recent.reduce((s, p) => ({ a: s.a + p.a, c: s.c + p.c }), { a: second.a, c: second.c });
    return { a: sum.a / n, c: sum.c / n };
  }

  const level = (ms: { a: number; c: number }) => ({ a: toDb(ms.a) + config.offsetDb, c: toDb(ms.c) + config.offsetDb });

  function reading(): SplReading | null {
    if (config.source === null || elapsed === 0) return null;
    return {
      source: config.source,
      calibrated: config.calibrated,
      fast: level(fast),
      slow: level(slow),
      leq1m: level(leq(60)),
      leq15m: level(leq(15 * 60)),
      leqTotal: level({ a: total.a / elapsed, c: total.c / elapsed }),
      aMax: toDb(aMax) + config.offsetDb,
      cPeak: toDb(cPeak) + config.offsetDb,
      seconds: elapsed,
    };
  }

  function clear() {
    points = [];
    elapsed = 0;
    total = { a: 0, c: 0 };
    fast = { a: 0, c: 0 };
    slow = { a: 0, c: 0 };
    second = { a: 0, c: 0, n: 0 };
    aMax = 0;
    cPeak = 0;
  }

  return {
    start() {
      if (timer) return;
      if (elapsed === 0) for (let i = 0; i < PREFILL_SECS * 10; i++) step(0.1);
      timer = setInterval(() => {
        if (config.source === null) return;
        step(TICK_MS / 1000);
        const r = reading();
        if (r) listeners.forEach((cb) => cb(r));
      }, TICK_MS);
    },
    stop() {
      if (timer) clearInterval(timer);
      timer = null;
      cancelCal();
    },
    getConfig: () => config,
    setConfig(next) {
      const offsetDb = Number.isFinite(next.offsetDb) ? Math.min(180, Math.max(60, next.offsetDb)) : DEFAULT_OFFSET_DB;
      if (next.source !== config.source) {
        clear();
        cancelCal();
      }
      // Only a calibration marks readings calibrated; typing an offset clears it.
      const typed = Math.abs(offsetDb - config.offsetDb) >= 0.05;
      const calibration = typed ? null : config.calibration;
      config = {
        source: next.source,
        offsetDb,
        calibration,
        calibrated: calibration !== null && calibration.source === next.source,
      };
      return config;
    },
    calibrate(weighting, referenceDb) {
      if (!(referenceDb >= 30 && referenceDb <= 140)) {
        throw new Error("Enter the reference meter's reading, between 30 and 140 dB.");
      }
      if (config.source === null) throw new Error("Choose the measurement input first.");
      if (!timer) throw new Error("Start Dante audio in Setup first, so there's something to measure.");
      cancelCal();
      let ticks = 0;
      const neededSecs = 5;
      calTimer = setInterval(() => {
        ticks++;
        const levelDbfs = toDb(weighting === "a" ? slow.a : slow.c);
        // The first half second lets the filters settle, like the real one.
        const steadySecs = Math.max(0, (ticks - 2) * 0.25);
        if (steadySecs < neededSecs) {
          emitCal({ state: "listening", levelDbfs, steadySecs, neededSecs, elapsedSecs: ticks * 0.25, hold: null });
          return;
        }
        cancelCal();
        const source = config.source!;
        config = {
          source,
          offsetDb: referenceDb - levelDbfs,
          calibrated: true,
          calibration: {
            source,
            device: "Dante Virtual Soundcard",
            sampleRate: 48000,
            weighting,
            referenceDb,
            atMs: Date.now(),
          },
        };
        emitCal({ state: "done", offsetDb: config.offsetDb, config });
      }, 250);
    },
    cancelCalibration: cancelCal,
    onCalibration(cb) {
      calListeners.add(cb);
      return () => calListeners.delete(cb);
    },
    reset: clear,
    reading,
    history(seconds) {
      return points.slice(-seconds).map((p) => ({ t: p.t, a: toDb(p.a) + config.offsetDb, c: toDb(p.c) + config.offsetDb }));
    },
    listen(cb) {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
  };
}
