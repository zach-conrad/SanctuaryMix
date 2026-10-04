import { Circle, Square, X } from "lucide-react";
import { audioModeLabel, formatBytes, formatClock } from "../lib/recordings";
import { useMixer } from "../store/mixer";
import { useRecordings } from "../store/recordings";
import { StatusPill } from "./StatusPill";

/** Record service / Recording 0:42:13 · Stop, in the top bar. */
export function RecordControl() {
  const recorder = useRecordings((s) => s.recorder);
  const busy = useRecordings((s) => s.recordBusy);
  const justSaved = useRecordings((s) => s.justSaved);
  const recordError = useRecordings((s) => s.recordError);
  const replaying = useRecordings((s) => s.replay?.state === "running");
  const replayId = useRecordings((s) => s.replay?.recordingId ?? null);
  const { startRecording, stopRecording, dismissSaved, open } = useRecordings();
  const ready = recorder !== null;
  const active = recorder?.active ?? null;

  return (
    <div className="record-control">
      {replaying && replayId && (
        <StatusPill tone="warn" subject="Replay" meta="Sending moves" onClick={() => void open(replayId)} />
      )}
      {recorder?.warning && <StatusPill tone="warn" subject="Recording" meta={recorder.warning} />}
      {active ? (
        <>
          <span className="rec-live" role="status" aria-live="off">
            <span className="rec-dot" aria-hidden />
            Recording
            <span className="text-readout rec-time">{formatClock(recorder!.elapsedMs)}</span>
          </span>
          <button className="sm-btn topbar-btn" onClick={() => void stopRecording()} disabled={busy}>
            <Square />
            Stop
          </button>
        </>
      ) : (
        <button
          className="sm-btn topbar-btn"
          onClick={() => void startRecording()}
          disabled={!ready || busy || replaying}
          title={
            replaying
              ? "Stop sending moves to the console before recording."
              : "Records the main mix and every fader and mute move"
          }
        >
          <Circle />
          Record service
        </button>
      )}
      {(justSaved || recordError) && (
        <div className="rec-popover" role="status">
          {justSaved ? (
            <>
              <div className="rec-popover-text">
                <span className="text-body-strong">Saved {justSaved.title}</span>
                <span className="text-caption muted">
                  <span className="text-readout">{formatClock(justSaved.durationMs)}</span> · {audioModeLabel(justSaved)}
                  {justSaved.bytesOnDisk > 0 && ` · ${formatBytes(justSaved.bytesOnDisk)}`}
                </span>
              </div>
              <button
                className="sm-btn sm-btn--sm"
                onClick={() => {
                  void open(justSaved.id);
                  dismissSaved();
                }}
              >
                Open service
              </button>
            </>
          ) : (
            <p className="error rec-popover-text">{recordError}</p>
          )}
          <button className="sm-btn sm-btn--ghost sm-btn--sm icon-btn" aria-label="Dismiss" title="Dismiss" onClick={dismissSaved}>
            <X />
          </button>
        </div>
      )}
    </div>
  );
}

/** Used by views that need to know a recording is running. */
export function useIsRecording(): boolean {
  return useRecordings((s) => !!s.recorder?.active);
}

/** The connected console's name for dialogs ("Practice console", "Allen & Heath dLive"). */
export function useConsoleName(): string | null {
  return useMixer((s) => (s.consoleStatus === "on" ? s.consoleModel : null));
}
