import { ChevronRight, CloudOff, HardDrive, Mic, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { Dialog } from "../components/Dialog";
import { useIsRecording } from "../components/RecordControl";
import { getBackend } from "../lib/backend";
import {
  STEREO_BYTES_PER_HOUR,
  SYNC_LABEL,
  audioModeLabel,
  bytesPerHour,
  estimatedTrackBytes,
  formatBytes,
  formatClock,
  formatGb,
  groupByServiceDate,
  oldMultitracks,
  requiredFreeBytes,
  serviceDateLabel,
  timeOfDay,
} from "../lib/recordings";
import type { RecordingSummary } from "../lib/types";
import { useMixer } from "../store/mixer";
import { useRecordings } from "../store/recordings";
import { ServiceDetail } from "./ServiceDetail";

const RETENTION_DAYS = 30;

export function RecordingsView() {
  const openId = useRecordings((s) => s.openId);
  return openId ? <ServiceDetail /> : <ServicesList />;
}

function ServicesList() {
  const list = useRecordings((s) => s.list);
  const loaded = useRecordings((s) => s.loaded);
  const error = useRecordings((s) => s.error);
  const refresh = useRecordings((s) => s.refresh);
  useEffect(() => {
    void refresh();
  }, [refresh]);
  const groups = groupByServiceDate(list);

  return (
    <div className="page services-page">
      <div className="services-main">
        <div className="services-head">
          <h1 className="text-title">Services</h1>
          <p className="muted">
            Every recorded service with its mix and each fader and mute move. Open one to listen and see what changed.
          </p>
        </div>
        {error && <p className="error">{error}</p>}
        {loaded && list.length === 0 && (
          <section className="panel">
            <p className="muted">
              No services yet. Press Record service in the top bar when the service starts; it saves when you press
              Stop.
            </p>
          </section>
        )}
        {groups.map((g) => (
          <section key={g.date} className="service-group" aria-label={serviceDateLabel(g.date)}>
            <h2 className="text-heading service-date">
              {serviceDateLabel(g.date)}
              <span className="text-caption muted">
                {g.items.length} {g.items.length === 1 ? "service" : "services"}
              </span>
            </h2>
            <ul className="service-list">
              {g.items.map((r) => (
                <li key={r.id}>
                  <ServiceRow r={r} />
                </li>
              ))}
            </ul>
          </section>
        ))}
      </div>
      <aside className="services-side">
        <DiskPanel />
        <RecordingSettingsPanel />
      </aside>
    </div>
  );
}

function ServiceRow({ r }: { r: RecordingSummary }) {
  const open = useRecordings((s) => s.open);
  return (
    <button className="service-row" onClick={() => void open(r.id)}>
      <span className="text-readout service-start">{timeOfDay(r.startedAt)}</span>
      <span className="service-title">
        <span className="service-title-line">
          <span className="text-body-strong">{r.title}</span>
          {r.status === "interrupted" && (
            <span className="sm-pill sm-pill--warn">
              <span className="sm-pill__dot" aria-hidden />
              Interrupted
            </span>
          )}
          {r.status === "recording" && (
            <span className="sm-pill sm-pill--error">
              <span className="sm-pill__dot" aria-hidden />
              Recording now
            </span>
          )}
        </span>
        {r.notes && <span className="text-caption muted service-notes">{r.notes}</span>}
      </span>
      <span className="text-readout service-duration" aria-label="Length">
        {formatClock(r.durationMs)}
      </span>
      <span className="text-caption service-what">{audioModeLabel(r)}</span>
      <span className="text-readout service-size muted" aria-label="Size">
        {formatBytes(r.bytesOnDisk)}
      </span>
      <span className="text-label muted service-sync" title="Saved on this computer. Cloud backup comes with team accounts.">
        <CloudOff size={20} strokeWidth={1.75} aria-hidden />
        {SYNC_LABEL[r.syncState]}
      </span>
      <ChevronRight size={20} strokeWidth={1.75} className="muted" aria-hidden />
    </button>
  );
}

function DiskPanel() {
  const disk = useRecordings((s) => s.disk);
  const list = useRecordings((s) => s.list);
  const role = useMixer((s) => s.session?.role);
  const deleteOld = useRecordings((s) => s.deleteOldMultitracks);
  const [confirming, setConfirming] = useState(false);
  const [freed, setFreed] = useState<number | null>(null);
  const [now] = useState(() => Date.now());
  const old = oldMultitracks(list, RETENTION_DAYS, now);
  const oldBytes = old.reduce((n, r) => n + estimatedTrackBytes(r), 0);
  const canDelete = role === "admin" || role === "engineer";
  const cutoff = new Date(now - RETENTION_DAYS * 86_400_000).toLocaleDateString("en-US", { month: "long", day: "numeric" });

  return (
    <section className="panel">
      <div className="panel-head">
        <HardDrive size={20} strokeWidth={1.75} />
        <h2 className="text-heading">Disk space</h2>
      </div>
      {disk ? (
        <dl className="disk-stats">
          <div>
            <dt className="text-caption muted">Free on this computer</dt>
            <dd className="text-readout-lg">{formatBytes(disk.freeBytes)}</dd>
          </div>
          <div className="disk-row">
            <dt className="text-caption muted">Service recordings</dt>
            <dd className="text-readout">{formatBytes(disk.recordingsBytes)}</dd>
          </div>
          <div className="disk-row">
            <dt className="text-caption muted">Multitracks in that</dt>
            <dd className="text-readout">{formatBytes(disk.multitrackBytes)}</dd>
          </div>
          <div className="disk-row">
            <dt className="text-caption muted">Room for stereo recording</dt>
            <dd className="text-readout">about {Math.floor(disk.freeBytes / STEREO_BYTES_PER_HOUR)} h</dd>
          </div>
        </dl>
      ) : (
        <p className="empty">Checking disk space…</p>
      )}
      {disk && <p className="text-caption muted folder" title={disk.folder}>{disk.folder}</p>}
      <div className="disk-action">
        <button
          className="sm-btn sm-btn--danger"
          disabled={!canDelete || old.length === 0}
          onClick={() => {
            setFreed(null);
            setConfirming(true);
          }}
        >
          <Trash2 />
          Delete multitracks older than {RETENTION_DAYS} days
        </button>
        <p className="text-caption muted">
          {!canDelete
            ? "Only engineers and admins can delete recordings."
            : old.length === 0
              ? `No multitracks are older than ${RETENTION_DAYS} days.`
              : `${old.length} ${old.length === 1 ? "service has" : "services have"} multitracks from before ${cutoff}, about ${formatBytes(oldBytes)}. The stereo mix and the moves stay.`}
        </p>
        {freed !== null && <p className="text-caption muted">Freed {formatBytes(freed)}.</p>}
      </div>
      {confirming && (
        <Dialog
          title="Delete old multitracks?"
          onClose={() => setConfirming(false)}
          actions={
            <>
              <button className="sm-btn sm-btn--ghost" onClick={() => setConfirming(false)} data-autofocus>
                Cancel
              </button>
              <button
                className="sm-btn sm-btn--danger"
                onClick={async () => {
                  setConfirming(false);
                  setFreed(await deleteOld(RETENTION_DAYS));
                }}
              >
                Delete multitracks
              </button>
            </>
          }
        >
          <p>
            This deletes the separate input tracks from {old.length} {old.length === 1 ? "service" : "services"} recorded
            before {cutoff}, about {formatBytes(oldBytes)}. Their stereo mix and the fader and mute moves stay.
          </p>
          <p className="muted">This can't be undone.</p>
        </Dialog>
      )}
    </section>
  );
}

function RecordingSettingsPanel() {
  const settings = useRecordings((s) => s.settings);
  const save = useRecordings((s) => s.saveSettings);
  const disk = useRecordings((s) => s.disk);
  const audio = useMixer((s) => s.audio);
  const recording = useIsRecording();
  const [deviceInputs, setDeviceInputs] = useState<number | null>(null);

  useEffect(() => {
    let live = true;
    void getBackend()
      .then((b) => b.listAudioDevices())
      .then((list) => {
        const d = list.find((x) => x.name === audio?.deviceName) ?? list.find((x) => x.isDante);
        if (live) setDeviceInputs(d?.maxInputChannels ?? null);
      })
      .catch(() => live && setDeviceInputs(null));
    return () => {
      live = false;
    };
  }, [audio?.deviceName]);

  if (!settings) return null;
  const inputs = Math.max(deviceInputs ?? 0, audio?.channels ?? 0, 2);
  const tracks = audio?.channels ?? deviceInputs ?? 0;
  const stereo = settings.mixChannels !== null;
  const [l, r] = settings.mixChannels ?? [null, null];
  const perHour = bytesPerHour(stereo, settings.multitrack ? tracks : 0);
  const need = requiredFreeBytes(stereo, settings.multitrack ? tracks : 0);
  const short = disk !== null && need > disk.freeBytes;

  const pick = (side: 0 | 1, value: string) => {
    if (value === "") return void save({ ...settings, mixChannels: null });
    const v = Number(value);
    const cur = settings.mixChannels ?? [v, Math.min(inputs - 1, v + 1)];
    const next: [number, number] = side === 0 ? [v, cur[1]] : [cur[0], v];
    void save({ ...settings, mixChannels: next });
  };

  const options = Array.from({ length: inputs }, (_, i) => (
    <option key={i} value={i}>
      Dante input {i + 1}
    </option>
  ));

  return (
    <section className="panel">
      <div className="panel-head">
        <Mic size={20} strokeWidth={1.75} />
        <h2 className="text-heading">Recording</h2>
      </div>
      <p className="muted">
        On the dLive, patch Main L and Main R to two Dante outputs, then pick the inputs they arrive on.
      </p>
      <div className="mix-inputs">
        <label className="sm-field">
          <span className="sm-field__label">Main L</span>
          <select className="sm-input" value={l ?? ""} onChange={(e) => pick(0, e.target.value)}>
            <option value="">Don't record audio</option>
            {options}
          </select>
        </label>
        <label className="sm-field">
          <span className="sm-field__label">Main R</span>
          <select className="sm-input" value={r ?? ""} onChange={(e) => pick(1, e.target.value)} disabled={!stereo}>
            {!stereo && <option value="">Don't record audio</option>}
            {options}
          </select>
        </label>
      </div>
      {!stereo && <p className="text-caption muted">Without audio, only the fader and mute moves are recorded.</p>}

      <div className="sm-field">
        <span className="sm-field__label" id="multitrack-label">Multitrack</span>
        <div className="sm-seg" role="group" aria-labelledby="multitrack-label">
          <button aria-pressed={!settings.multitrack} onClick={() => void save({ ...settings, multitrack: false })}>
            Off
          </button>
          <button
            aria-pressed={settings.multitrack}
            disabled={!stereo}
            onClick={() => void save({ ...settings, multitrack: true })}
          >
            Every input
          </button>
        </div>
        <span className="sm-field__help">
          {settings.multitrack
            ? `About ${formatGb(tracks * bytesPerHour(false, 1))} per hour for ${tracks} inputs, plus ${formatGb(STEREO_BYTES_PER_HOUR)} for the stereo mix.`
            : stereo
              ? `The stereo mix is about ${formatGb(STEREO_BYTES_PER_HOUR)} per hour. Multitrack adds about ${formatGb(tracks * bytesPerHour(false, 1))} per hour for ${tracks} inputs.`
              : "Pick Main L and Main R to record audio."}
        </span>
      </div>

      {stereo && (
        <p className={`text-caption ${short ? "warning-text" : "muted"}`}>
          {short
            ? `Not enough free space to start: this needs ${formatGb(need)}. Delete old multitracks or turn multitrack off.`
            : `A 75-minute service is about ${formatGb(perHour * 1.25)}.`}
        </p>
      )}
      {recording && <p className="text-caption muted">Changes apply to the next recording.</p>}
    </section>
  );
}
