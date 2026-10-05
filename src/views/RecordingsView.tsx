import { ChevronRight, CloudOff, Trash2 } from "lucide-react";
import { can } from "../lib/plans";
import { useEffect, useState } from "react";
import { Dialog } from "../components/Dialog";
import { PageHeader } from "../components/PageHeader";
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
    <div className="page">
      <PageHeader title="Services" />
      <div className="page-split">
        <div className="page-main">
          {error && <p className="error">{error}</p>}
          {loaded && list.length === 0 && (
            <div className="group">
              <div className="row empty">No services yet. Press Record service to start.</div>
            </div>
          )}
          {groups.map((g) => (
            <section key={g.date} className="section" aria-label={serviceDateLabel(g.date)}>
              <div className="section-head">
                <h2>{serviceDateLabel(g.date)}</h2>
                <span className="text-caption muted">
                  {g.items.length} {g.items.length === 1 ? "service" : "services"}
                </span>
              </div>
              <ul className="group service-list">
                {g.items.map((r) => (
                  <li key={r.id}>
                    <ServiceRow r={r} />
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </div>
        <aside className="page-side">
          <RecordingSettingsPanel />
          <DiskPanel />
        </aside>
      </div>
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
      <span className="service-facts">
        <span className="text-readout" aria-label="Length">
          {formatClock(r.durationMs)}
        </span>
        <span className="text-caption muted">
          {audioModeLabel(r)} · <span aria-label="Size">{formatBytes(r.bytesOnDisk)}</span>
        </span>
      </span>
      <span
        className="text-caption muted service-sync"
        title={
          r.syncState === "localOnly"
            ? "Saved on this computer. Cloud backup comes with team accounts."
            : SYNC_LABEL[r.syncState]
        }
      >
        <CloudOff size={20} strokeWidth={1.75} aria-hidden />
        {/* Every service is local today, so the icon alone says it; other states show their word. */}
        <span className={r.syncState === "localOnly" ? "visually-hidden" : undefined}>{SYNC_LABEL[r.syncState]}</span>
      </span>
      <ChevronRight size={20} strokeWidth={1.75} className="muted" aria-hidden />
    </button>
  );
}

function DiskPanel() {
  const disk = useRecordings((s) => s.disk);
  const list = useRecordings((s) => s.list);
  const canDelete = useMixer((s) => can(s.session, "deleteRecordings"));
  const deleteOld = useRecordings((s) => s.deleteOldMultitracks);
  const [confirming, setConfirming] = useState(false);
  const [freed, setFreed] = useState<number | null>(null);
  const [now] = useState(() => Date.now());
  const old = oldMultitracks(list, RETENTION_DAYS, now);
  const oldBytes = old.reduce((n, r) => n + estimatedTrackBytes(r), 0);
  const cutoff = new Date(now - RETENTION_DAYS * 86_400_000).toLocaleDateString("en-US", {
    month: "long",
    day: "numeric",
  });

  return (
    <section className="section" aria-labelledby="disk-title">
      <div className="section-head">
        <h2 id="disk-title">Disk space</h2>
      </div>
      <dl className="group">
        {disk ? (
          <>
            <div className="row">
              <dt className="row-text">Free</dt>
              <dd className="text-readout">{formatBytes(disk.freeBytes)}</dd>
            </div>
            <div className="row">
              <dt className="row-text">Recordings</dt>
              <dd className="text-readout">{formatBytes(disk.recordingsBytes)}</dd>
            </div>
            <div className="row">
              <dt className="row-text">Multitracks</dt>
              <dd className="text-readout">{formatBytes(disk.multitrackBytes)}</dd>
            </div>
            <div className="row">
              <dt className="row-text">Stereo time left</dt>
              <dd className="text-readout">{Math.floor(disk.freeBytes / STEREO_BYTES_PER_HOUR)} h</dd>
            </div>
          </>
        ) : (
          <div className="row empty">Checking disk space…</div>
        )}
        <div className="row">
          <button
            className="sm-btn sm-btn--danger disk-delete"
            disabled={!canDelete || old.length === 0}
            title={!canDelete ? "Only engineers and admins can delete recordings." : undefined}
            onClick={() => {
              setFreed(null);
              setConfirming(true);
            }}
          >
            <Trash2 />
            Delete old multitracks
          </button>
        </div>
      </dl>
      <p className="section-foot">
        {freed !== null
          ? `Freed ${formatBytes(freed)}.`
          : old.length === 0
            ? `None older than ${RETENTION_DAYS} days.`
            : `${old.length} older than ${RETENTION_DAYS} days, ${formatBytes(oldBytes)}. Stereo mixes stay.`}
      </p>
      {disk && (
        <p className="section-foot folder" title={disk.folder}>
          {disk.folder}
        </p>
      )}
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
            This deletes the separate input tracks from {old.length} {old.length === 1 ? "service" : "services"}{" "}
            recorded before {cutoff}, about {formatBytes(oldBytes)}. Their stereo mix and the fader and mute moves stay.
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
      Input {i + 1}
    </option>
  ));

  return (
    <section className="section" aria-labelledby="recording-title">
      <div className="section-head">
        <h2 id="recording-title">Recording</h2>
      </div>
      <div className="group">
        <label className="row" title="Patch Main L/R to two Dante outputs.">
          <span className="row-text">Main L</span>
          <select className="sm-input" value={l ?? ""} onChange={(e) => pick(0, e.target.value)}>
            <option value="">No audio</option>
            {options}
          </select>
        </label>
        <label className="row">
          <span className="row-text">Main R</span>
          <select className="sm-input" value={r ?? ""} onChange={(e) => pick(1, e.target.value)} disabled={!stereo}>
            {!stereo && <option value="">No audio</option>}
            {options}
          </select>
        </label>
        <div className="row">
          <span className="row-text" id="multitrack-label">
            Multitrack
          </span>
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
        </div>
      </div>
      <p className={`section-foot${short ? " warning-text" : ""}`}>
        {!stereo
          ? "Moves only, no audio."
          : short
            ? `Not enough space: needs ${formatGb(need)}.`
            : `${formatGb(perHour)} per hour.`}
        {recording && " Applies to the next recording."}
      </p>
    </section>
  );
}
