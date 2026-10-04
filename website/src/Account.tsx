import { ChevronRight, CircleCheck, Download, Info, Laptop, Monitor, Music } from "lucide-react";
import { useEffect, useState } from "react";
import { audioModeLabel, formatClock, serviceDateLabel, timeOfDay } from "../../src/lib/recordings";
import { connectCloud, type CloudRecording } from "./cloud";
import { Footer } from "./components/Footer";
import { Header } from "./components/Header";
import { FOUNDING_OFFER, TRIAL_DAYS, findPlan, formatPrice, foundingPrice, type Billing } from "./pricing";
import { downloadUrl, fetchLatestRelease, type ReleaseLookup } from "./release";

const ROOT = "../";

const dateFormat = new Intl.DateTimeFormat("en-US", { month: "long", day: "numeric", year: "numeric" });

function formatSize(bytes: number) {
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/** The plan picked on the pricing section, passed in the page's query string. */
function chosenPlan() {
  const params = new URLSearchParams(window.location.search);
  const plan = findPlan(params.get("plan"));
  if (!plan) return null;
  const billing: Billing = params.get("billing") === "monthly" ? "monthly" : "yearly";
  return { plan, billing };
}

export function Account() {
  const [choice] = useState(chosenPlan);
  const [lookup, setLookup] = useState<ReleaseLookup>({ status: "loading" });
  const release = lookup.status === "ok" ? lookup.release : null;
  const notYet = lookup.status === "none";

  useEffect(() => {
    const ctrl = new AbortController();
    fetchLatestRelease(ROOT, ctrl.signal).then((r) => {
      if (!ctrl.signal.aborted) setLookup(r);
    });
    return () => ctrl.abort();
  }, []);

  return (
    <>
      <Header root={ROOT} signedIn />
      <main className="section">
        <div className="site-container account">
          <p className="preview-note text-caption" role="note">
            <Info aria-hidden="true" />
            This is a preview account page. Sign-in isn't built yet, so everyone sees this sample account.
          </p>

          {choice ? (
            <section className="plan-picked" aria-labelledby="picked-heading">
              <CircleCheck aria-hidden="true" />
              <div>
                <h2 id="picked-heading" className="text-heading">
                  Your {TRIAL_DAYS}-day {choice.plan.name} trial has started
                </h2>
                <p className="feature__body">
                  After the trial it's{" "}
                  {choice.billing === "yearly"
                    ? `${formatPrice(choice.plan.yearly)} a year`
                    : `${formatPrice(choice.plan.monthly)} a month`}
                  {FOUNDING_OFFER
                    ? `, or ${formatPrice(
                        foundingPrice(choice.billing === "yearly" ? choice.plan.yearly : choice.plan.monthly),
                      )} with the founding church offer`
                    : ""}
                  . Checkout isn't built yet, so nothing is charged. <a href={`${ROOT}#pricing`}>Change plan</a>
                </p>
              </div>
            </section>
          ) : null}

          <h1 className="account__title">{choice ? "Welcome, Alex" : "Welcome back, Alex"}</h1>
          <p className="section__lede">Download SanctuaryMix for your booth computer, and listen back to your services.</p>

          <div className="account__grid">
            <section className="panel panel--download" aria-labelledby="mac-heading">
              <div className="panel__head">
                <Laptop aria-hidden="true" />
                <h2 id="mac-heading" className="text-heading">SanctuaryMix for macOS</h2>
              </div>
              <p className="feature__body">Universal app for Apple silicon and Intel Macs, macOS 12 or later.</p>
              <dl className="release-meta">
                <div>
                  <dt className="text-label">Version</dt>
                  <dd className={release ? "text-readout" : undefined}>{release ? release.version : notYet ? "Coming soon" : "Checking"}</dd>
                </div>
                {release?.publishedAt ? (
                  <div>
                    <dt className="text-label">Released</dt>
                    <dd className="text-readout">{dateFormat.format(release.publishedAt)}</dd>
                  </div>
                ) : null}
                {release?.mac.size ? (
                  <div>
                    <dt className="text-label">Size</dt>
                    <dd className="text-readout">{formatSize(release.mac.size)}</dd>
                  </div>
                ) : null}
              </dl>
              {release ? (
                <>
                  <a className="sm-btn sm-btn--primary sm-btn--lg" href={downloadUrl(ROOT, release.mac.file)} download>
                    <Download aria-hidden="true" />
                    Download for Mac
                  </a>
                  <p className="text-caption panel__foot">
                    Open the .dmg and drag SanctuaryMix to Applications. Early builds aren't signed yet: the first
                    time, Control-click the app and choose Open.
                  </p>
                </>
              ) : (
                <>
                  <button className="sm-btn sm-btn--primary sm-btn--lg" disabled>
                    <Download aria-hidden="true" />
                    Download for Mac
                  </button>
                  {notYet ? (
                    <p className="text-caption panel__foot">
                      The first build is on its way. This button turns on as soon as it's published.
                    </p>
                  ) : null}
                </>
              )}
            </section>

            <section className="panel" aria-labelledby="win-heading">
              <div className="panel__head">
                <Monitor aria-hidden="true" />
                <h2 id="win-heading" className="text-heading">SanctuaryMix for Windows</h2>
              </div>
              <p className="feature__body">Planned after the Mac version settles. We'll let you know here when it's ready.</p>
              <span className="sm-pill">
                <span className="sm-pill__dot" aria-hidden="true" />
                Planned
              </span>
            </section>

            <section className="panel" aria-labelledby="acct-heading">
              <h2 id="acct-heading" className="text-heading">Your account</h2>
              <dl className="account-details">
                <div>
                  <dt className="text-caption">Name</dt>
                  <dd>Alex Rivera</dd>
                </div>
                <div>
                  <dt className="text-caption">Church</dt>
                  <dd>Grace Community Church</dd>
                </div>
                <div>
                  <dt className="text-caption">Role</dt>
                  <dd>Engineer</dd>
                </div>
                <div>
                  <dt className="text-caption">Plan</dt>
                  <dd>
                    {choice ? `${choice.plan.name}, ${choice.billing} (trial)` : "Early access"}
                  </dd>
                </div>
              </dl>
            </section>
          </div>

          <Services />
        </div>
      </main>
      <Footer />
    </>
  );
}

const cloud = connectCloud(ROOT);

/** Services the church's booth computers have uploaded. */
function Services() {
  const [list, setList] = useState<CloudRecording[] | null>(null);
  const [error, setError] = useState(false);

  useEffect(() => {
    cloud.listRecordings().then(setList, () => setError(true));
  }, []);

  return (
    <section id="services" className="services" aria-labelledby="services-heading">
      <div className="services__head">
        <h2 id="services-heading" className="section__title">Recorded services</h2>
        <p className="feature__body">
          Mixes from your booth, with every fader and mute move. Listen back here or share a link.
        </p>
      </div>
      {error ? <p className="mix-muted">Your services didn't load. Reload the page to try again.</p> : null}
      {list && list.length === 0 ? (
        <p className="mix-muted">No services yet. Recordings appear here after the booth computer uploads them.</p>
      ) : null}
      {list && list.length > 0 ? (
        <ul className="services__list">
          {list.map((r) => (
            <li key={r.id}>
              <a className="service-row" href={`${ROOT}account/service/?id=${encodeURIComponent(r.id)}`}>
                <Music aria-hidden="true" />
                <span className="service-row__main">
                  <span className="text-body-strong">{r.title}</span>
                  <span className="text-caption mix-muted">
                    {serviceDateLabel(r.serviceDate)} · {timeOfDay(r.startedAt)} · {audioModeLabel(r)} ·{" "}
                    {r.eventCount} moves
                  </span>
                </span>
                <span className="text-readout mix-muted">{formatClock(r.durationMs)}</span>
                <ChevronRight aria-hidden="true" />
              </a>
            </li>
          ))}
        </ul>
      ) : null}
    </section>
  );
}
