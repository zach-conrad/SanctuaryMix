import { CircleCheck, Download, Info, Laptop, Monitor } from "lucide-react";
import { useEffect, useState } from "react";
import { Footer } from "./components/Footer";
import { Header } from "./components/Header";
import { TRIAL_DAYS, findPlan, formatPrice, type Billing } from "./pricing";
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
                  . Checkout isn't built yet, so nothing is charged. <a href={`${ROOT}#pricing`}>Change plan</a>
                </p>
              </div>
            </section>
          ) : null}

          <h1 className="account__title">{choice ? "Welcome, Alex" : "Welcome back, Alex"}</h1>
          <p className="section__lede">Download the latest SanctuaryMix for your booth computer.</p>

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
        </div>
      </main>
      <Footer />
    </>
  );
}
