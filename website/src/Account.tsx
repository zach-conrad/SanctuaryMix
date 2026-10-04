import { Download, Info, Laptop, Monitor } from "lucide-react";
import { useEffect, useState } from "react";
import { Footer } from "./components/Footer";
import { Header } from "./components/Header";
import { ASSETS, downloadUrl, fetchLatestRelease, releasesPageUrl, type LatestRelease } from "./release";

const ROOT = "../";

const dateFormat = new Intl.DateTimeFormat("en-US", { month: "long", day: "numeric", year: "numeric" });

function formatSize(bytes: number) {
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function Account() {
  const [release, setRelease] = useState<LatestRelease | null | undefined>(undefined);

  useEffect(() => {
    const ctrl = new AbortController();
    fetchLatestRelease(ctrl.signal).then((r) => {
      if (!ctrl.signal.aborted) setRelease(r);
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

          <h1 className="account__title">Welcome back, Alex</h1>
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
                  <dd className={release ? "text-readout" : undefined}>{release ? release.version : "Latest build"}</dd>
                </div>
                <div>
                  <dt className="text-label">Released</dt>
                  <dd className={release ? "text-readout" : undefined}>{release ? dateFormat.format(release.publishedAt) : "With each new build"}</dd>
                </div>
                {release?.sizeBytes ? (
                  <div>
                    <dt className="text-label">Size</dt>
                    <dd className="text-readout">{formatSize(release.sizeBytes)}</dd>
                  </div>
                ) : null}
              </dl>
              <a className="sm-btn sm-btn--primary sm-btn--lg" href={downloadUrl(ASSETS.mac)}>
                <Download aria-hidden="true" />
                Download for Mac
              </a>
              <p className="text-caption panel__foot">
                Open the .dmg and drag SanctuaryMix to Applications.{" "}
                <a href={releasesPageUrl}>All releases</a>
              </p>
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
                  <dd>Early access</dd>
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
