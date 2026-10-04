import { Check, RefreshCw } from "lucide-react";
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
    <section className="section" aria-labelledby="audio-title">
      <div className="section-head">
        <h2 id="audio-title">Dante audio</h2>
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
      {access === "denied" ? (
        <div className="group">
          <div className="row" role="status">
            <span className="row-text">
              <span className="text-body-strong">Microphone access is off</span>
              <span className="text-caption">Turn on SanctuaryMix in Privacy &amp; Security › Microphone.</span>
            </span>
            <button className="sm-btn sm-btn--ghost" onClick={refresh} disabled={loading}>
              Check again
            </button>
            <button className="sm-btn" onClick={openSettings}>
              Open Settings
            </button>
          </div>
        </div>
      ) : (
        <div className="group" role="radiogroup" aria-label="Audio input">
          {devices.map((d) => (
            <label key={d.name} className="row">
              <input
                className="visually-hidden"
                type="radio"
                name="device"
                checked={selected === d.name}
                onChange={() => setSelected(d.name)}
              />
              <span className="row-text">
                <span className="text-body-strong">{d.name}</span>
                <span className="text-caption">
                  {d.maxInputChannels} {d.maxInputChannels === 1 ? "input" : "inputs"} · {d.defaultSampleRate / 1000}{" "}
                  kHz{d.isDante ? " · Dante" : d.isDefault ? " · System default" : ""}
                </span>
              </span>
              {selected === d.name && <Check className="row-check" size={20} strokeWidth={2} aria-hidden />}
            </label>
          ))}
          {access === null && <div className="row empty">Checking audio inputs…</div>}
          {access === "granted" && devices.length === 0 && !listError && (
            <div className="row empty">No audio inputs. Is Dante Virtual Soundcard running?</div>
          )}
          <div className="row row--actions">
            <span className="muted">
              {running && audio
                ? `Listening · ${audio.channels} ch · ${audio.sampleRate / 1000} kHz`
                : "Route dLive inputs 1:1 to Dante Virtual Soundcard."}
            </span>
            {running ? (
              <button className="sm-btn" onClick={stopAudio}>
                Stop listening
              </button>
            ) : (
              <button
                className="sm-btn sm-btn--primary"
                disabled={!selected || access !== "granted" || audioStatus === "connecting"}
                onClick={() => startAudio(selected)}
              >
                {audioStatus === "connecting" ? "Starting…" : "Start listening"}
              </button>
            )}
          </div>
        </div>
      )}
      {listError && <p className="error section-foot">{listError}</p>}
      {audioError && <p className="error section-foot">{audioError}</p>}
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
    <section className="section" aria-labelledby="console-title">
      <div className="section-head">
        <h2 id="console-title">Console</h2>
      </div>
      <div className="group">
        <label className="row">
          <span className="row-text">Console</span>
          <select
            className="sm-input"
            value={form.model}
            onChange={(e) => update({ model: e.target.value as ConsoleConfig["model"] })}
          >
            <option value="dlive">Allen &amp; Heath dLive</option>
            <option value="simulated">Practice console</option>
          </select>
        </label>
        {form.model === "dlive" && (
          <>
            <label className="row" data-invalid={hostTouched && !!hostProblem}>
              <span className="row-text">
                <span>IP address</span>
                <span className={`text-caption${hostTouched && hostProblem ? " error" : ""}`}>
                  {hostTouched && hostProblem ? hostProblem : "Utility › Control › Network"}
                </span>
              </span>
              <input
                className="sm-input"
                value={form.host}
                inputMode="decimal"
                placeholder="192.168.1.70"
                onChange={(e) => update({ host: e.target.value.trim() })}
                onBlur={() => setHostTouched(true)}
              />
            </label>
            <label className="row">
              <span className="row-text">
                <span>MIDI channel</span>
                <span className="text-caption">Utility › Control › MIDI</span>
              </span>
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
            </label>
          </>
        )}
        <label className="row">
          <span className="row-text">Inputs to show</span>
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
        <div className="row row--actions">
          <span className="muted">{connected ? `Connected to ${consoleModel}` : "Not connected"}</span>
          {connected ? (
            <button className="sm-btn sm-btn--danger" onClick={disconnectConsole}>
              Disconnect
            </button>
          ) : (
            <button
              className="sm-btn sm-btn--primary"
              disabled={consoleStatus === "connecting" || !!hostProblem}
              onClick={() => connectConsole(form)}
            >
              {consoleStatus === "connecting" ? "Connecting…" : "Connect"}
            </button>
          )}
        </div>
      </div>
      {consoleError && <p className="error section-foot">{consoleError}</p>}
    </section>
  );
}
