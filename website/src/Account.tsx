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

  const plan = choice ? `${choice.plan.name}, ${choice.billing}` : "Early access";
  const afterTrial = choice
    ? choice.billing === "yearly"
      ? `${formatPrice(choice.plan.yearly)} a year`
      : `${formatPrice(choice.plan.monthly)} a month`
    : "";
  const founding = choice && FOUNDING_OFFER
    ? formatPrice(foundingPrice(choice.billing === "yearly" ? choice.plan.yearly : choice.plan.monthly))
    : null;
  const versionLine = release
    ? [
        `Version ${release.version}`,
        release.publishedAt ? dateFormat.format(release.publishedAt) : null,
        release.mac.size ? formatSize(release.mac.size) : null,
      ]
        .filter(Boolean)
        .join(" · ")
    : notYet
      ? "Coming soon"
      : "Checking for the latest version";

  return (
    <>
      <Header root={ROOT} signedIn />
      <main className="section section--tight">
        <div className="site-container">
          <header className="page-header">
            <h1 className="page-title">{choice ? "Welcome, Alex" : "Welcome back, Alex"}</h1>
          </header>

          {choice ? (
            <section className="grouped" aria-labelledby="picked-heading">
              <div className="group">
                <div className="row">
                  <span className="row-icon row-icon--accent">
                    <CircleCheck aria-hidden="true" />
                  </span>
                  <span className="row-text">
                    <span id="picked-heading">
                      {choice.plan.name} trial started · {TRIAL_DAYS} days
                    </span>
                    <span className="text-caption">
                      Then {afterTrial}
                      {founding ? `, or ${founding} with the founding church offer` : ""}. Nothing is charged yet.
                    </span>
                  </span>
                  <a className="sm-btn" href={`${ROOT}#pricing`}>
                    Change plan
                  </a>
                </div>
              </div>
            </section>
          ) : null}

          <div className="page-split">
            <div className="page-main">
              <section className="grouped" aria-labelledby="download-heading">
                <div className="section-head">
                  <h2 id="download-heading">Download</h2>
                </div>
                <ul className="group">
                  <li className="row">
                    <span className="row-icon">
                      <Laptop aria-hidden="true" />
                    </span>
                    <span className="row-text">
                      <span>macOS</span>
                      <span className="text-caption">{versionLine}</span>
                    </span>
                    {release ? (
                      <a className="sm-btn sm-btn--primary sm-btn--lg" href={downloadUrl(ROOT, release.mac.file)} download>
                        <Download aria-hidden="true" />
                        Download
                      </a>
                    ) : (
                      <button className="sm-btn sm-btn--primary sm-btn--lg" disabled>
                        <Download aria-hidden="true" />
                        Download
                      </button>
                    )}
                  </li>
                  <li className="row">
                    <span className="row-icon">
                      <Monitor aria-hidden="true" />
                    </span>
                    <span className="row-text">
                      <span>Windows</span>
                      <span className="text-caption">After the Mac version settles</span>
                    </span>
                    <span className="sm-pill">
                      <span className="sm-pill__dot" aria-hidden="true" />
                      Planned
                    </span>
                  </li>
                </ul>
                <p className="section-foot">
                  {release
                    ? "Early builds aren't signed yet. The first time, Control-click the app and choose Open."
                    : notYet
                      ? "The button turns on when the first build is published."
                      : "Universal app for Apple silicon and Intel Macs, macOS 12 or later."}
                </p>
              </section>

              <Services />
            </div>

            <aside className="page-side">
              <section className="grouped" aria-labelledby="acct-heading">
                <div className="section-head">
                  <h2 id="acct-heading">Account</h2>
                </div>
                <dl className="group">
                  <div className="row">
                    <dt className="row-text">Name</dt>
                    <dd className="row-value">Alex Rivera</dd>
                  </div>
                  <div className="row">
                    <dt className="row-text">Church</dt>
                    <dd className="row-value">Grace Community Church</dd>
                  </div>
                  <div className="row">
                    <dt className="row-text">Role</dt>
                    <dd className="row-value">Engineer</dd>
                  </div>
                  <div className="row">
                    <dt className="row-text">Plan</dt>
                    <dd className="row-value">{choice ? `${plan} (trial)` : plan}</dd>
                  </div>
                </dl>
                <p className="section-foot footnote">
                  <Info aria-hidden="true" />
                  Preview account. Sign-in isn't built yet.
                </p>
              </section>
            </aside>
          </div>
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

  const days = groupByDate(list ?? []);

  return (
    <section id="services" className="services" aria-labelledby="services-heading">
      <div className="section-head">
        <h2 id="services-heading">Recorded services</h2>
        {list ? <span className="text-caption mix-muted">{list.length}</span> : null}
      </div>
      {error ? <p className="section-foot">Your services didn't load. Reload the page to try again.</p> : null}
      {list && list.length === 0 ? (
        <div className="empty-state">
          <Music aria-hidden="true" />
          <p className="text-heading">No services yet</p>
          <p className="text-caption mix-muted">They appear here after the booth computer uploads them.</p>
        </div>
      ) : null}
      {days.map(([date, items]) => (
        <section key={date} className="grouped" aria-label={serviceDateLabel(date)}>
          <div className="section-head section-head--sub">
            <h3>{serviceDateLabel(date)}</h3>
          </div>
          <ul className="group">
            {items.map((r) => (
              <li key={r.id}>
                <a className="row row--link" href={`${ROOT}account/service/?id=${encodeURIComponent(r.id)}`}>
                  <span className="row-text">
                    <span>{r.title}</span>
                    <span className="text-caption">
                      {timeOfDay(r.startedAt)} · {audioModeLabel(r)} · {r.eventCount} moves
                    </span>
                  </span>
                  <span className="text-readout mix-muted">{formatClock(r.durationMs)}</span>
                  <ChevronRight aria-hidden="true" className="row-chevron" />
                </a>
              </li>
            ))}
          </ul>
        </section>
      ))}
    </section>
  );
}

/** One group per service date, newest first, as the app's Services page does (groupByServiceDate). */
function groupByDate(list: CloudRecording[]): [string, CloudRecording[]][] {
  const days = new Map<string, CloudRecording[]>();
  for (const r of [...list].sort((a, b) => b.startedAt - a.startedAt)) {
    days.set(r.serviceDate, [...(days.get(r.serviceDate) ?? []), r]);
  }
  return [...days.entries()].sort(([a], [b]) => (a < b ? 1 : a > b ? -1 : 0));
}
