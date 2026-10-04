import { AudioLines, Router } from "lucide-react";
import { useEffect, useState } from "react";
import { getBackend } from "../lib/backend";
import type { AudioDeviceInfo, ConsoleConfig } from "../lib/types";
import { useMixer } from "../store/mixer";

export function SetupView() {
  return (
    <div className="page setup">
      <AudioCard />
      <ConsoleCard />
    </div>
  );
}

function AudioCard() {
  const { audioStatus, audio, audioError, startAudio, stopAudio } = useMixer();
  const [devices, setDevices] = useState<AudioDeviceInfo[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [listError, setListError] = useState<string | null>(null);

  const refresh = async () => {
    try {
      const list = await (await getBackend()).listAudioDevices();
      setDevices(list);
      setSelected((cur) => cur ?? list[0]?.name ?? null);
      setListError(null);
    } catch (e) {
      setListError(String(e));
    }
  };
  useEffect(() => {
    void refresh();
  }, []);

  const running = audioStatus === "on";
  return (
    <section className="card">
      <div className="card-head">
        <AudioLines size={20} />
        <h2>Audio input</h2>
        <button className="ghost" onClick={refresh}>
          Refresh
        </button>
      </div>
      <p className="muted">
        Choose <strong>Dante Virtual Soundcard</strong> and patch the dLive's input channels to it in Dante Controller,
        one to one (console input 1 to DVS input 1).
      </p>
      {listError && <p className="error">{listError}</p>}
      <div className="device-list" role="radiogroup">
        {devices.map((d) => (
          <label key={d.name} className={`device ${selected === d.name ? "selected" : ""}`}>
            <input type="radio" name="device" checked={selected === d.name} onChange={() => setSelected(d.name)} />
            <div>
              <strong>{d.name}</strong>
              <span>
                {d.maxInputChannels} inputs · {d.defaultSampleRate / 1000} kHz
              </span>
            </div>
            {d.isDante && <span className="tag">Dante</span>}
            {d.isDefault && <span className="tag tag-quiet">System default</span>}
          </label>
        ))}
        {devices.length === 0 && !listError && <p className="empty">No audio inputs found.</p>}
      </div>
      {audioError && <p className="error">{audioError}</p>}
      <div className="actions">
        {running && audio && (
          <span className="muted">
            Listening to {audio.channels} channels at {audio.sampleRate / 1000} kHz
          </span>
        )}
        {running ? (
          <button className="secondary" onClick={stopAudio}>
            Stop
          </button>
        ) : (
          <button className="primary" disabled={!selected || audioStatus === "connecting"} onClick={() => startAudio(selected)}>
            {audioStatus === "connecting" ? "Starting…" : "Start listening"}
          </button>
        )}
      </div>
    </section>
  );
}

function ConsoleCard() {
  const { consoleStatus, consoleModel, consoleError, consoleConfig, connectConsole, disconnectConsole } = useMixer();
  const [form, setForm] = useState<ConsoleConfig>(consoleConfig);
  const update = (patch: Partial<ConsoleConfig>) => setForm((f) => ({ ...f, ...patch }));
  const connected = consoleStatus === "on";

  return (
    <section className="card">
      <div className="card-head">
        <Router size={20} />
        <h2>Console</h2>
      </div>
      <div className="form">
        <label>
          <span>Console</span>
          <select value={form.model} onChange={(e) => update({ model: e.target.value as ConsoleConfig["model"] })}>
            <option value="dlive">Allen &amp; Heath dLive</option>
            <option value="simulated">Simulated console (no hardware)</option>
          </select>
        </label>
        {form.model === "dlive" && (
          <>
            <label>
              <span>MixRack or Surface IP</span>
              <input value={form.host} onChange={(e) => update({ host: e.target.value.trim() })} placeholder="192.168.1.70" />
            </label>
            <label>
              <span>MIDI channel</span>
              <select value={form.midiChannel} onChange={(e) => update({ midiChannel: Number(e.target.value) })}>
                {Array.from({ length: 12 }, (_, i) => (
                  <option key={i} value={i}>
                    {i + 1}
                  </option>
                ))}
              </select>
            </label>
          </>
        )}
        <label>
          <span>Input channels to show</span>
          <input
            type="number"
            min={1}
            max={128}
            value={form.inputCount}
            onChange={(e) => update({ inputCount: Math.max(1, Math.min(128, Number(e.target.value) || 1)) })}
          />
        </label>
      </div>
      {form.model === "dlive" && (
        <p className="muted small">
          On the dLive, the MIDI channel is set under Utility › Control › MIDI. SanctuaryMix connects to TCP port 51328 on
          the same network as the console's network port.
        </p>
      )}
      {consoleError && <p className="error">{consoleError}</p>}
      <div className="actions">
        {connected && <span className="muted">Connected to {consoleModel}</span>}
        {connected ? (
          <button className="secondary" onClick={disconnectConsole}>
            Disconnect
          </button>
        ) : (
          <button
            className="primary"
            disabled={consoleStatus === "connecting" || (form.model === "dlive" && !form.host)}
            onClick={() => connectConsole(form)}
          >
            {consoleStatus === "connecting" ? "Connecting…" : "Connect"}
          </button>
        )}
      </div>
    </section>
  );
}
