import { AudioLines, RefreshCw, Router } from "lucide-react";
import { useEffect, useState } from "react";
import { getBackend } from "../lib/backend";
import type { AudioDeviceInfo, ConsoleConfig, MicAccess } from "../lib/types";
import { PageHeader } from "../components/PageHeader";
import { useMixer } from "../store/mixer";

export function SetupView() {
  return (
    <div className="page page--narrow">
      <PageHeader title="Setup" />
      <ConsolePanel />
      <AudioPanel />
    </div>
  );
}

/**
 * Lists inputs only once microphone access is settled. On macOS, touching
 * audio inputs before the user answers (or after they say no) brings the
 * permission prompt back, so this asks once and shares the answer. The shared
 * promise also covers React running the mount effect twice in development.
 */
let pendingLoad: Promise<AudioLoad> | null = null;

type AudioLoad = { access: MicAccess; devices: AudioDeviceInfo[] };

function loadAudioInputs(): Promise<AudioLoad> {
  pendingLoad ??= (async () => {
    const backend = await getBackend();
    let access = await backend.microphoneAccess();
    if (access === "undetermined") access = await backend.requestMicrophoneAccess();
    const devices = access === "granted" ? await backend.listAudioDevices() : [];
    return { access, devices };
  })().finally(() => {
    pendingLoad = null;
  });
  return pendingLoad;
}

function AudioPanel() {
  const { audioStatus, audio, audioError, startAudio, stopAudio } = useMixer();
  const [access, setAccess] = useState<MicAccess | null>(null);
  const [devices, setDevices] = useState<AudioDeviceInfo[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [listError, setListError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const refresh = async () => {
    setLoading(true);
    try {
      const result = await loadAudioInputs();
      setAccess(result.access);
      setDevices(result.devices);
      setSelected((cur) => cur ?? result.devices[0]?.name ?? null);
      setListError(null);
    } catch (e) {
      setListError(`Couldn't list audio inputs. ${String(e)}`);
    } finally {
      setLoading(false);
    }
  };
  useEffect(() => {
    void refresh();
  }, []);

  const openSettings = async () => {
    try {
      await (await getBackend()).openMicrophoneSettings();
    } catch (e) {
      setListError(String(e));
    }
  };

  const running = audioStatus === "on";
  return (
    <section className="panel">
      <div className="panel-head">
        <AudioLines size={20} strokeWidth={1.75} />
        <h2 className="text-heading">Dante audio</h2>
        <button
          className="sm-btn sm-btn--ghost sm-btn--sm"
          onClick={refresh}
          disabled={loading}
          aria-label="Refresh audio inputs"
          title="Refresh"
        >
          <RefreshCw />
        </button>
      </div>
      <p className="text-caption muted">Route dLive inputs 1:1 to Dante Virtual Soundcard.</p>
      {listError && <p className="error">{listError}</p>}
      {access === "denied" ? (
        <div className="permission-off" role="status">
          <p className="text-body-strong">Microphone access is off</p>
          <p className="muted">Turn on SanctuaryMix in Privacy &amp; Security › Microphone.</p>
          <div className="actions">
            <button className="sm-btn sm-btn--lg sm-btn--ghost" onClick={refresh} disabled={loading}>
              Check again
            </button>
            <button className="sm-btn sm-btn--lg" onClick={openSettings}>
              Open System Settings
            </button>
          </div>
        </div>
      ) : (
        <div className="device-list" role="radiogroup" aria-label="Audio input">
          {devices.map((d) => (
            <label key={d.name} className="device" data-selected={selected === d.name}>
              <input type="radio" name="device" checked={selected === d.name} onChange={() => setSelected(d.name)} />
              <span className="device-text">
                <span className="text-body-strong">{d.name}</span>
                <span className="text-caption muted">
                  {d.maxInputChannels} {d.maxInputChannels === 1 ? "input" : "inputs"} · {d.defaultSampleRate / 1000}{" "}
                  kHz
                </span>
              </span>
              {d.isDante && <span className="text-label muted">Dante</span>}
              {d.isDefault && !d.isDante && <span className="text-label muted">System default</span>}
            </label>
          ))}
          {access === null && <p className="empty">Checking audio inputs…</p>}
          {access === "granted" && devices.length === 0 && !listError && (
            <p className="empty">No audio inputs found. Is Dante Virtual Soundcard running?</p>
          )}
        </div>
      )}
      {audioError && <p className="error">{audioError}</p>}
      <div className="actions">
        {running && audio && (
          <span className="muted">
            Listening to {audio.channels} channels at {audio.sampleRate / 1000} kHz
          </span>
        )}
        {running ? (
          <button className="sm-btn sm-btn--lg" onClick={stopAudio}>
            Stop listening
          </button>
        ) : (
          <button
            className="sm-btn sm-btn--lg sm-btn--primary"
            disabled={!selected || access !== "granted" || audioStatus === "connecting"}
            onClick={() => startAudio(selected)}
          >
            {audioStatus === "connecting" ? "Starting…" : "Start listening"}
          </button>
        )}
      </div>
    </section>
  );
}

/** Common input counts, plus the saved one if it is something else. */
const INPUT_COUNTS = [8, 16, 24, 32, 48, 64, 96, 128];
function inputCountOptions(current: number): number[] {
  return INPUT_COUNTS.includes(current) ? INPUT_COUNTS : [...INPUT_COUNTS, current].sort((a, b) => a - b);
}

const IPV4 = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/;

function ipError(host: string): string | null {
  const m = IPV4.exec(host);
  if (!m) return "Enter the console's IP address, like 192.168.1.70.";
  if (m.slice(1).some((n) => Number(n) > 255)) return "Each number must be 0 to 255.";
  return null;
}

function ConsolePanel() {
  const { consoleStatus, consoleModel, consoleError, consoleConfig, connectConsole, disconnectConsole } = useMixer();
  const [form, setForm] = useState<ConsoleConfig>(consoleConfig);
  const [hostTouched, setHostTouched] = useState(false);
  const update = (patch: Partial<ConsoleConfig>) => setForm((f) => ({ ...f, ...patch }));
  const connected = consoleStatus === "on";
  const hostProblem = form.model === "dlive" ? ipError(form.host) : null;

  return (
    <section className="panel">
      <div className="panel-head">
        <Router size={20} strokeWidth={1.75} />
        <h2 className="text-heading">Console</h2>
      </div>
      <div className="form">
        <label className="sm-field">
          <span className="sm-field__label">Console</span>
          <select
            className="sm-input"
            value={form.model}
            onChange={(e) => update({ model: e.target.value as ConsoleConfig["model"] })}
          >
            <option value="dlive">Allen &amp; Heath dLive</option>
            <option value="simulated">Practice console (no hardware)</option>
          </select>
        </label>
        {form.model === "dlive" && (
          <>
            <label className="sm-field" data-invalid={hostTouched && !!hostProblem}>
              <span className="sm-field__label">MixRack or Surface IP address</span>
              <input
                className="sm-input"
                value={form.host}
                inputMode="decimal"
                placeholder="192.168.1.70"
                onChange={(e) => update({ host: e.target.value.trim() })}
                onBlur={() => setHostTouched(true)}
              />
              <span className="sm-field__help">
                {hostTouched && hostProblem ? hostProblem : "dLive: Utility › Control › Network"}
              </span>
            </label>
            <label className="sm-field">
              <span className="sm-field__label">MIDI channel</span>
              <select
                className="sm-input"
                value={form.midiChannel}
                onChange={(e) => update({ midiChannel: Number(e.target.value) })}
              >
                {Array.from({ length: 12 }, (_, i) => (
                  <option key={i} value={i}>
                    {i + 1}
                  </option>
                ))}
              </select>
              <span className="sm-field__help">dLive: Utility › Control › MIDI</span>
            </label>
          </>
        )}
        <label className="sm-field">
          <span className="sm-field__label">Inputs to show</span>
          <select
            className="sm-input"
            value={form.inputCount}
            onChange={(e) => update({ inputCount: Number(e.target.value) })}
          >
            {inputCountOptions(form.inputCount).map((n) => (
              <option key={n} value={n}>
                {n} inputs
              </option>
            ))}
          </select>
        </label>
      </div>
      {consoleError && <p className="error">{consoleError}</p>}
      <div className="actions">
        {connected && <span className="muted">Connected to {consoleModel}</span>}
        {connected ? (
          <button className="sm-btn sm-btn--lg sm-btn--danger" onClick={disconnectConsole}>
            Disconnect
          </button>
        ) : (
          <button
            className="sm-btn sm-btn--lg sm-btn--primary"
            disabled={consoleStatus === "connecting" || !!hostProblem}
            onClick={() => connectConsole(form)}
          >
            {consoleStatus === "connecting" ? "Connecting…" : "Connect"}
          </button>
        )}
      </div>
    </section>
  );
}
