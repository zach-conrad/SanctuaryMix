import { CloudOff, Info, Link2Off } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { audioModeLabel, formatClock, serviceDateLabel, timeOfDay } from "../../src/lib/recordings";
import { connectCloud, type SharedMix } from "./cloud";
import { Footer } from "./components/Footer";
import { Wordmark } from "./components/Wordmark";
import { MixPlayer } from "./mix/MixPlayer";

const ROOT = "../";

const dateFormat = new Intl.DateTimeFormat("en-US", { month: "long", day: "numeric", year: "numeric" });

const GONE: Record<Exclude<SharedMix["status"], "ok">, { title: string; body: string }> = {
  expired: { title: "This link has ended", body: "Ask the person who shared it for a new link." },
  revoked: { title: "This link was turned off", body: "Ask the person who shared it for a new link." },
  notFound: { title: "This link doesn't work", body: "Check that you copied the whole link, or ask for a new one." },
};

/** Public playback page for a share link. No sign-in, no account details. */
export function Share() {
  const cloud = useMemo(() => connectCloud(ROOT), []);
  const [shared, setShared] = useState<SharedMix | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    const token = new URLSearchParams(window.location.search).get("t") ?? "";
    let live = true;
    cloud
      .openShare(token)
      .then((s) => live && setShared(s))
      .catch(() => live && setFailed(true));
    return () => {
      live = false;
    };
  }, [cloud]);

  useEffect(() => {
    if (shared?.status === "ok") document.title = `${shared.recording.title} · ${shared.orgName} · SanctuaryMix`;
  }, [shared]);

  return (
    <>
      <header className="site-header">
        <div className="site-container site-header__inner">
          <a href={ROOT} className="site-header__home" aria-label="SanctuaryMix home">
            <Wordmark root={ROOT} />
          </a>
          <span className="text-caption mix-muted">Shared service mix</span>
        </div>
      </header>
      <main className="section section--tight">
        <div className="site-container">
          {!shared && !failed ? <p className="mix-muted">Opening the mix</p> : null}
          {failed ? (
            <div className="empty-state">
              <CloudOff aria-hidden="true" />
              <h1 className="text-heading">This mix didn't load</h1>
              <p className="text-caption mix-muted">Check your connection and reload the page.</p>
            </div>
          ) : null}
          {shared && shared.status !== "ok" ? (
            <div className="empty-state">
              <Link2Off aria-hidden="true" />
              <h1 className="text-heading">{GONE[shared.status].title}</h1>
              <p className="text-caption mix-muted">{GONE[shared.status].body}</p>
            </div>
          ) : null}
          {shared?.status === "ok" ? (
            <>
              <header className="page-header">
                <div className="page-header__text">
                  <h1 className="page-title">{shared.recording.title}</h1>
                  <p className="text-caption mix-muted">
                    {shared.orgName} · {serviceDateLabel(shared.recording.serviceDate)} ·{" "}
                    {timeOfDay(shared.recording.startedAt)} ·{" "}
                    <span className="text-readout">{formatClock(shared.recording.durationMs)}</span> ·{" "}
                    {audioModeLabel(shared.recording)}
                    {shared.expiresAt ? ` · Link works until ${dateFormat.format(shared.expiresAt)}` : ""}
                  </p>
                </div>
              </header>
              <div className="service__layout">
                <MixPlayer audioUrl={shared.audioUrl} durationMs={shared.recording.durationMs} events={shared.events} />
                {shared.previewNote ? (
                  <p className="section-foot footnote" role="note">
                    <Info aria-hidden="true" />
                    {shared.previewNote}
                  </p>
                ) : null}
              </div>
            </>
          ) : null}
        </div>
      </main>
      <Footer />
    </>
  );
}
