// The one place the UI talks to the Rust core. Inside the app this calls Tauri
// commands; in a plain browser (`npm run dev`) it falls back to a demo backend
// so the UI can be designed and reviewed without a Mac build or a console.

import type {
  Adjustment,
  AiEqConfig,
  AiEqStatus,
  ChannelEq,
  CompareSide,
  EqAudit,
  EqIdea,
  EqLogEntry,
  IdeaState,
  AudioDeviceInfo,
  AutoMixConfig,
  AutoMixStatus,
  ChannelRole,
  HeardChannel,
  Preset,
  ChannelId,
  ConsoleConfig,
  ConsoleEvent,
  MeterFrame,
  MeteringInfo,
  MicAccess,
  Session,
  DiskUsage,
  OutputDeviceInfo,
  PlaybackStatus,
  RecordedEvent,
  RecorderStatus,
  RecordingDetail,
  RecordingSettings,
  RecordingSummary,
  ReplayStatus,
  CalibrationStatus,
  SplConfig,
  SplPoint,
  SplReading,
  Weighting,
} from "./types";
import { createDemoBackend } from "./demoBackend";

type Unlisten = () => void;

export interface Backend {
  readonly isDemo: boolean;
  microphoneAccess(): Promise<MicAccess>;
  /** Shows the macOS prompt if the user hasn't answered yet. */
  requestMicrophoneAccess(): Promise<MicAccess>;
  openMicrophoneSettings(): Promise<void>;
  listAudioDevices(): Promise<AudioDeviceInfo[]>;
  startMetering(device: string | null): Promise<MeteringInfo>;
  stopMetering(): Promise<void>;
  connectConsole(config: ConsoleConfig): Promise<void>;
  disconnectConsole(): Promise<void>;
  setFader(id: ChannelId, db: number | null): Promise<void>;
  setMute(id: ChannelId, muted: boolean): Promise<void>;
  getSession(): Promise<Session>;
  /** Opens the website to sign in. The result arrives through onSession or onSessionError. */
  beginSignIn(): Promise<void>;
  /** Sign-in changed outside a call: the website handed it back, or the plan was re-checked between services. */
  onSession(cb: (session: Session) => void): Promise<Unlisten>;
  /** Website sign-in didn't finish; a sentence to show. */
  onSessionError(cb: (message: string) => void): Promise<Unlisten>;
  /** Back to the local session. Refused mid-service (recording or auto-mix on). */
  signOut(): Promise<Session>;
  /** Opens the website account page, where billing lives. */
  openBilling(): Promise<void>;

  // Auto-mix. The guardrails are enforced in the Rust core; these only ask.
  automixPresets(): Promise<Preset[]>;
  automixGuessRoles(names: string[]): Promise<ChannelRole[]>;
  automixGetConfig(): Promise<AutoMixConfig>;
  /** Returns the settings as the core will use them (clamped to its limits). */
  automixSetConfig(config: AutoMixConfig): Promise<AutoMixConfig>;
  automixEngage(on: boolean): Promise<void>;
  /** Stops every automatic move immediately. */
  automixFreeze(): Promise<void>;
  automixResume(): Promise<void>;
  automixResumeChannel(channel: number): Promise<void>;
  automixUndo(channel: number): Promise<void>;
  automixUndoAll(): Promise<void>;
  automixStatus(): Promise<AutoMixStatus>;
  /** Newest first. */
  automixLog(limit?: number): Promise<Adjustment[]>;
  /** Listens to every input with signal for `seconds`, to suggest roles by ear. */
  automixListenScan(seconds: number): Promise<void>;
  /** What each input has mostly sounded like. `names` are the console channel names, by index. */
  automixHeard(names: string[]): Promise<HeardChannel[]>;
  onAutomix(cb: (status: AutoMixStatus) => void): Promise<Unlisten>;
  onAutomixAdjustment(cb: (adjustment: Adjustment) => void): Promise<Unlisten>;

  // AI EQ (docs/AIEQ.md). Hard limits live in the Rust core; these only ask.
  aieqGetConfig(): Promise<AiEqConfig>;
  /** Engineers and Admins for tone keeping and the tap point; anyone for on/off. */
  aieqSetConfig(config: AiEqConfig): Promise<AiEqConfig>;
  aieqStatus(): Promise<AiEqStatus>;
  onAiEq(cb: (status: AiEqStatus) => void): Promise<Unlisten>;
  onAiEqLog(cb: (entry: EqLogEntry) => void): Promise<Unlisten>;
  /** Newest first. */
  aieqLog(limit?: number): Promise<EqLogEntry[]>;
  /** Starts listening to every picked channel for soundcheck. Refused mid-service. */
  aieqSoundcheckStart(): Promise<void>;
  aieqSoundcheckStop(): Promise<void>;
  /** Applies a channel's soundcheck proposal to the desk. */
  aieqApply(channel: number): Promise<void>;
  aieqApplyAll(): Promise<void>;
  aieqSkip(channel: number): Promise<void>;
  /** Adopts the desk's EQ as it is; AI EQ only keeps band 4 for feedback. Engineers and Admins. */
  aieqKeepMyEq(): Promise<void>;
  /** Puts a channel's EQ back to its soundcheck EQ and holds it there. */
  aieqUndo(channel: number): Promise<void>;
  aieqUndoAll(): Promise<void>;
  aieqHandBack(channel: number): Promise<void>;
  /** A person's own EQ edit from the EQ panel. Marks the channel's EQ as theirs. */
  aieqSetEq(channel: number, eq: ChannelEq): Promise<void>;
  /** Flips the desk between the EQ before and after the last apply. Not during a service. */
  aieqCompare(channel: number, side: CompareSide | null): Promise<void>;
  /** Puts last Sunday's EQ back on the channels that differ. */
  aieqRestoreProfile(): Promise<void>;
  aieqDismissProfile(): Promise<void>;
  /** Feedback check: Engineers and Admins; the UI asks first, every time. */
  aieqRingOutStart(): Promise<void>;
  aieqRingOutStop(): Promise<void>;
  aieqDismissFeedback(): Promise<void>;
  /** Ideas for next week; `recordingId` null for every waiting idea. */
  aieqIdeas(recordingId: string | null): Promise<EqIdea[]>;
  /** Keep or dismiss an idea. Engineers and Admins. */
  aieqSetIdea(id: string, state: IdeaState): Promise<EqIdea[]>;
  /** Every EQ change around a recorded service, plus its ideas. */
  aieqAudit(recordingId: string): Promise<EqAudit>;

  onMeters(cb: (frame: MeterFrame) => void): Promise<Unlisten>;
  onConsole(cb: (event: ConsoleEvent) => void): Promise<Unlisten>;

  // Service recordings (docs/RECORDINGS.md)
  getRecordingSettings(): Promise<RecordingSettings>;
  setRecordingSettings(settings: RecordingSettings): Promise<void>;
  /** Starts recording the current service. Audio follows the saved settings. */
  startRecording(title: string | null): Promise<RecordingSummary>;
  stopRecording(): Promise<RecordingSummary | null>;
  recorderStatus(): Promise<RecorderStatus>;
  listRecordings(): Promise<RecordingSummary[]>;
  getRecording(id: string): Promise<RecordingDetail>;
  recordingEvents(id: string): Promise<RecordedEvent[]>;
  updateRecording(id: string, title: string, notes: string): Promise<RecordingSummary>;
  deleteRecording(id: string): Promise<void>;
  diskUsage(): Promise<DiskUsage>;
  /** Deletes multitrack files older than `days`; returns bytes freed. Mix and moves are kept. */
  deleteOldMultitracks(days: number): Promise<number>;
  onRecorder(cb: (status: RecorderStatus) => void): Promise<Unlisten>;

  // Playback
  listOutputDevices(): Promise<OutputDeviceInfo[]>;
  loadPlayback(id: string, device: string | null): Promise<PlaybackStatus>;
  play(): Promise<void>;
  pause(): Promise<void>;
  seek(positionMs: number): Promise<void>;
  unloadPlayback(): Promise<void>;
  onPlayback(cb: (status: PlaybackStatus) => void): Promise<Unlisten>;

  // Replay to console (guarded; Engineer and Admin only)
  startReplay(id: string, fromMs: number, channels: ChannelId[] | null): Promise<ReplayStatus>;
  stopReplay(): Promise<ReplayStatus>;
  restoreBeforeReplay(): Promise<ReplayStatus>;
  onReplay(cb: (status: ReplayStatus) => void): Promise<Unlisten>;

  // Room loudness (A and C weighted SPL from one measurement input)
  splGetConfig(): Promise<SplConfig>;
  /** Returns the settings as the core will use them. A new source starts over. */
  splSetConfig(config: SplConfig): Promise<SplConfig>;
  /**
   * Starts a guided calibration: listens until the level holds steady, then sets
   * the offset so it reads `referenceDb`. Progress arrives through `onSplCalibration`.
   */
  splCalibrate(weighting: Weighting, referenceDb: number): Promise<void>;
  splCancelCalibration(): Promise<void>;
  onSplCalibration(cb: (status: CalibrationStatus) => void): Promise<Unlisten>;
  /** Clears history, Leq, maximum and peak. */
  splReset(): Promise<void>;
  splReading(): Promise<SplReading | null>;
  /** One point per second for the last `seconds`, oldest first. */
  splHistory(seconds: number): Promise<SplPoint[]>;
  onSpl(cb: (reading: SplReading) => void): Promise<Unlisten>;
}

function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function createTauriBackend(): Promise<Backend> {
  const { invoke } = await import("@tauri-apps/api/core");
  const { listen } = await import("@tauri-apps/api/event");
  return {
    isDemo: false,
    microphoneAccess: () => invoke("microphone_access"),
    requestMicrophoneAccess: () => invoke("request_microphone_access"),
    openMicrophoneSettings: () => invoke("open_microphone_settings"),
    listAudioDevices: () => invoke("list_audio_devices"),
    startMetering: (device) => invoke("start_metering", { device }),
    stopMetering: () => invoke("stop_metering"),
    connectConsole: (config) => invoke("connect_console", { config }),
    disconnectConsole: () => invoke("disconnect_console"),
    setFader: (id, db) => invoke("set_fader", { id, db }),
    setMute: (id, muted) => invoke("set_mute", { id, muted }),
    getSession: () => invoke("get_session"),
    beginSignIn: () => invoke("begin_sign_in"),
    onSession: (cb) => listen<Session>("session", (e) => cb(e.payload)),
    onSessionError: (cb) => listen<string>("session-error", (e) => cb(e.payload)),
    signOut: () => invoke("sign_out"),
    openBilling: () => invoke("open_billing"),
    automixPresets: () => invoke("automix_presets"),
    automixGuessRoles: (names) => invoke("automix_guess_roles", { names }),
    automixGetConfig: () => invoke("automix_get_config"),
    automixSetConfig: (config) => invoke("automix_set_config", { config }),
    automixEngage: (on) => invoke("automix_engage", { on }),
    automixFreeze: () => invoke("automix_freeze"),
    automixResume: () => invoke("automix_resume"),
    automixResumeChannel: (channel) => invoke("automix_resume_channel", { channel }),
    automixUndo: (channel) => invoke("automix_undo", { channel }),
    automixUndoAll: () => invoke("automix_undo_all"),
    automixStatus: () => invoke("automix_status"),
    automixLog: (limit) => invoke("automix_log", { limit: limit ?? null }),
    automixListenScan: (seconds) => invoke("automix_listen_scan", { seconds }),
    automixHeard: (names) => invoke("automix_heard", { names }),
    onAutomix: (cb) => listen<AutoMixStatus>("automix", (e) => cb(e.payload)),
    onAutomixAdjustment: (cb) => listen<Adjustment>("automix-adjustment", (e) => cb(e.payload)),
    aieqGetConfig: () => invoke("aieq_get_config"),
    aieqSetConfig: (config) => invoke("aieq_set_config", { config }),
    aieqStatus: () => invoke("aieq_status"),
    onAiEq: (cb) => listen<AiEqStatus>("aieq", (e) => cb(e.payload)),
    onAiEqLog: (cb) => listen<EqLogEntry>("aieq-log", (e) => cb(e.payload)),
    aieqLog: (limit) => invoke("aieq_log", { limit: limit ?? null }),
    aieqSoundcheckStart: () => invoke("aieq_soundcheck_start"),
    aieqSoundcheckStop: () => invoke("aieq_soundcheck_stop"),
    aieqApply: (channel) => invoke("aieq_apply", { channel }),
    aieqApplyAll: () => invoke("aieq_apply_all"),
    aieqSkip: (channel) => invoke("aieq_skip", { channel }),
    aieqKeepMyEq: () => invoke("aieq_keep_my_eq"),
    aieqUndo: (channel) => invoke("aieq_undo", { channel }),
    aieqUndoAll: () => invoke("aieq_undo_all"),
    aieqHandBack: (channel) => invoke("aieq_hand_back", { channel }),
    aieqSetEq: (channel, eq) => invoke("aieq_set_eq", { channel, eq }),
    aieqCompare: (channel, side) => invoke("aieq_compare", { channel, side }),
    aieqRestoreProfile: () => invoke("aieq_restore_profile"),
    aieqDismissProfile: () => invoke("aieq_dismiss_profile"),
    aieqRingOutStart: () => invoke("aieq_ring_out_start"),
    aieqRingOutStop: () => invoke("aieq_ring_out_stop"),
    aieqDismissFeedback: () => invoke("aieq_dismiss_feedback"),
    aieqIdeas: (recordingId) => invoke("aieq_ideas", { recordingId }),
    aieqSetIdea: (id, state) => invoke("aieq_set_idea", { id, state }),
    aieqAudit: (recordingId) => invoke("aieq_audit", { recordingId }),
    onMeters: (cb) => listen<MeterFrame>("meters", (e) => cb(e.payload)),
    onConsole: (cb) => listen<ConsoleEvent>("console", (e) => cb(e.payload)),

    getRecordingSettings: () => invoke("get_recording_settings"),
    setRecordingSettings: (settings) => invoke("set_recording_settings", { settings }),
    startRecording: (title) => invoke("start_recording", { title }),
    stopRecording: () => invoke("stop_recording"),
    recorderStatus: () => invoke("recorder_status"),
    listRecordings: () => invoke("list_recordings"),
    getRecording: (id) => invoke("get_recording", { id }),
    recordingEvents: (id) => invoke("recording_events", { id }),
    updateRecording: (id, title, notes) => invoke("update_recording", { id, title, notes }),
    deleteRecording: (id) => invoke("delete_recording", { id }),
    diskUsage: () => invoke("disk_usage"),
    deleteOldMultitracks: (days) => invoke("delete_old_multitracks", { days }),
    onRecorder: (cb) => listen<RecorderStatus>("recording", (e) => cb(e.payload)),

    listOutputDevices: () => invoke("list_output_devices"),
    loadPlayback: (id, device) => invoke("load_playback", { id, device }),
    play: () => invoke("play_recording"),
    pause: () => invoke("pause_recording"),
    seek: (positionMs) => invoke("seek_recording", { positionMs }),
    unloadPlayback: () => invoke("unload_playback"),
    onPlayback: (cb) => listen<PlaybackStatus>("playback", (e) => cb(e.payload)),

    startReplay: (id, fromMs, channels) => invoke("start_replay", { id, fromMs, channels }),
    stopReplay: () => invoke("stop_replay"),
    restoreBeforeReplay: () => invoke("restore_before_replay"),
    onReplay: (cb) => listen<ReplayStatus>("replay", (e) => cb(e.payload)),

    splGetConfig: () => invoke("spl_get_config"),
    splSetConfig: (config) => invoke("spl_set_config", { config }),
    splCalibrate: (weighting, referenceDb) => invoke("spl_calibrate", { weighting, referenceDb }),
    splCancelCalibration: () => invoke("spl_cancel_calibration"),
    onSplCalibration: (cb) => listen<CalibrationStatus>("spl-calibration", (e) => cb(e.payload)),
    splReset: () => invoke("spl_reset"),
    splReading: () => invoke("spl_reading"),
    splHistory: (seconds) => invoke("spl_history", { seconds }),
    onSpl: (cb) => listen<SplReading>("spl", (e) => cb(e.payload)),
  };
}

let backend: Promise<Backend> | null = null;

export function getBackend(): Promise<Backend> {
  backend ??= isTauri() ? createTauriBackend() : Promise.resolve(createDemoBackend());
  return backend;
}
