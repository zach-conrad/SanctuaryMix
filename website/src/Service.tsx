import { ArrowLeft, Info, Link2, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { audioModeLabel, formatClock, serviceDateLabel, timeOfDay } from "../../src/lib/recordings";
import { connectCloud, type CloudRecording, type RecordedEvent } from "./cloud";
import { Footer } from "./components/Footer";
import { Header } from "./components/Header";
import { MixPlayer } from "./mix/MixPlayer";
import { SharePanel } from "./mix/SharePanel";

const ROOT = "../../";

type Load =
  | { status: "loading" }
  | { status: "missing" }
  | { status: "error"; message: string }
  | { status: "ok"; recording: CloudRecording; events: RecordedEvent[]; audioUrl: string | null };

/** One recorded service from the church's cloud library: listen, watch the moves, share. */
export function Service() {
  const cloud = useMemo(() => connectCloud(ROOT), []);
  const [load, setLoad] = useState<Load>({ status: "loading" });
  const shareDialog = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const id = new URLSearchParams(window.location.search).get("id");
    let live = true;
    (async () => {
      const recording = id ? await cloud.getRecording(id) : null;
      if (!recording) return { status: "missing" } as const;
      const [events, audioUrl] = await Promise.all([cloud.getEvents(recording), cloud.listenUrl(recording)]);
      return { status: "ok", recording, events, audioUrl } as const;
    })()
      .catch((e: unknown) => ({ status: "error", message: e instanceof Error ? e.message : String(e) }) as const)
      .then((r) => {
        if (live) setLoad(r);
      });
    return () => {
      live = false;
    };
  }, [cloud]);

  useEffect(() => {
    if (load.status === "ok") document.title = `${load.recording.title} · SanctuaryMix`;
  }, [load]);

  return (
    <>
      <Header root={ROOT} signedIn />
      <main className="section section--tight">
        <div className="site-container">
          <a className="back-link" href={`${ROOT}account/#services`}>
            <ArrowLeft aria-hidden="true" />
            All services
          </a>

          {load.status === "loading" ? <p className="mix-muted service__status">Loading the service</p> : null}
          {load.status === "missing" ? (
            <div className="service__status">
              <h1 className="account__title">Service not found</h1>
              <p className="section__lede">It may have been deleted. Go back to your services and pick another.</p>
            </div>
          ) : null}
          {load.status === "error" ? (
            <div className="service__status">
              <h1 className="account__title">This service didn't load</h1>
              <p className="section__lede">{load.message}. Check your connection and reload the page.</p>
            </div>
          ) : null}

          {load.status === "ok" ? (
            <>
              <header className="service__head">
                <p className="text-caption mix-muted">
                  {serviceDateLabel(load.recording.serviceDate)} · {timeOfDay(load.recording.startedAt)} ·{" "}
                  <span className="text-readout">{formatClock(load.recording.durationMs)}</span> ·{" "}
                  {audioModeLabel(load.recording)}
                </p>
                <div className="service__title-row">
                  <h1 className="account__title">{load.recording.title}</h1>
                  <button className="sm-btn sm-btn--lg" onClick={() => shareDialog.current?.showModal()}>
                    <Link2 aria-hidden="true" />
                    Share
                  </button>
                </div>
                {load.recording.notes ? <p className="section__lede">{load.recording.notes}</p> : null}
              </header>

              <p className="preview-note text-caption service__note" role="note">
                <Info aria-hidden="true" />
                Playback only. To send these moves to a console, open the service in the SanctuaryMix app.
              </p>

              <div className="service__layout">
                <MixPlayer audioUrl={load.audioUrl} durationMs={load.recording.durationMs} events={load.events} />
              </div>

              <dialog
                ref={shareDialog}
                className="dialog"
                aria-labelledby="share-heading"
                onClick={(e) => {
                  // A click on the backdrop (the dialog element itself) closes it.
                  if (e.target === e.currentTarget) e.currentTarget.close();
                }}
              >
                <SharePanel cloud={cloud} recordingId={load.recording.id} root={ROOT} />
                <button
                  className="sm-btn sm-btn--ghost dialog__close"
                  onClick={() => shareDialog.current?.close()}
                  aria-label="Close"
                  title="Close"
                >
                  <X aria-hidden="true" />
                </button>
              </dialog>
            </>
          ) : null}
        </div>
      </main>
      <Footer />
    </>
  );
}
