import type { Backend } from "./backend";
import type { ConsoleEvent, MeterFrame } from "./types";

const NAMES = [
  "Pastor", "Worship Ld", "BGV 1", "BGV 2", "Kick", "Snare", "Hat", "Tom 1", "Tom 2",
  "OH L", "OH R", "Bass DI", "Elec Gtr", "Acous Gtr", "Keys L", "Keys R", "Pad L", "Pad R",
  "Choir L", "Choir R", "Handheld 1", "Handheld 2", "Video L", "Video R", "Playback L",
  "Playback R", "Lapel 1", "Lapel 2", "Ambient L", "Ambient R", "Spare 1", "Spare 2",
];

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
  const emit = (e: ConsoleEvent) => consoleListeners.forEach((cb) => cb(e));
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
        meterListeners.forEach((cb) => cb(frame));
      }, 33);
      return { deviceName: device ?? "Dante Virtual Soundcard", channels, sampleRate: 48000 };
    },
    async stopMetering() {
      if (timer) clearInterval(timer);
      timer = null;
    },
    async connectConsole(config) {
      await new Promise((r) => setTimeout(r, 400));
      emit({ type: "connected", model: config.model === "dlive" ? "Allen & Heath dLive (demo)" : "Practice console" });
      for (let i = 0; i < config.inputCount; i++) {
        emit({ type: "name", id: { kind: "input", index: i }, name: NAMES[i] ?? `Ch ${i + 1}` });
      }
    },
    async disconnectConsole() {
      emit({ type: "disconnected", reason: null });
    },
    async setFader(id, db) {
      emit({ type: "fader", id, db });
    },
    async setMute(id, muted) {
      emit({ type: "mute", id, muted });
    },
    async getSession() {
      return {
        user: { id: "local", displayName: "Local operator", email: null },
        activeOrg: null,
        role: "admin",
        authenticated: false,
      };
    },
    async onMeters(cb) {
      meterListeners.add(cb);
      return () => meterListeners.delete(cb);
    },
    async onConsole(cb) {
      consoleListeners.add(cb);
      return () => consoleListeners.delete(cb);
    },
  };
}
