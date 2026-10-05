import { ChevronRight, CircleCheck, CloudOff, Download, Laptop, Monitor, Music } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { audioModeLabel, formatClock, serviceDateLabel, timeOfDay } from "../../src/lib/recordings";
import { AuthForm, ChurchForm, NewPasswordForm } from "./auth/AuthForms";
import { cloudSession, signOut, useAccount, type Account as SignedIn } from "./auth/useAccount";
import { connectCloud, type CloudRecording, type RecordingsCloud } from "./cloud";
import { Footer } from "./components/Footer";
import { Header } from "./components/Header";
import { FOUNDING_OFFER, PLANS, formatPrice, foundingPrice } from "./pricing";
import { downloadUrl, fetchLatestRelease, type ReleaseLookup } from "./release";

const ROOT = "../";

const dateFormat = new Intl.DateTimeFormat("en-US", { month: "long", day: "numeric", year: "numeric" });

function formatSize(bytes: number) {
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

const ROLE_LABEL = { admin: "Admin", engineer: "Engineer", volunteer: "Volunteer" } as const;

const DAY_MS = 86_400_000;

/** Signed out: the sign-in form. Signed in: downloads, services and the account. */
export function Account() {
  const { state, refresh } = useAccount();
  const user = state.status === "ready" ? { name: state.account.name, church: state.account.church.name } : null;

  return (
    <>
      <Header root={ROOT} account user={user} onSignOut={() => void signOut(ROOT)} />
      <main className="section section--tight">
        <div className="site-container">
          {state.status === "loading" ? <p className="mix-muted service__status">Checking your account</p> : null}
          {state.status === "signedOut" ? <AuthForm root={ROOT} notice={state.notice} /> : null}
          {state.status === "recovery" ? <NewPasswordForm onDone={() => void refresh()} /> : null}
          {state.status === "needsChurch" ? <ChurchForm name={state.name} onDone={() => void refresh()} /> : null}
          {state.status === "error" ? (
            <div className="empty-state">
              <CloudOff aria-hidden="true" />
              <h1 className="text-heading">Your account didn't load</h1>
              <p className="text-caption mix-muted">{state.message} Reload the page to try again.</p>
              <button type="button" className="sm-btn sm-btn--ghost" onClick={() => void signOut(ROOT)}>
                Sign out
              </button>
            </div>
          ) : null}
          {state.status === "ready" ? <AccountHome account={state.account} /> : null}
        </div>
      </main>
      <Footer />
    </>
  );
}

function AccountHome({ account }: { account: SignedIn }) {
  const { church } = account;
  const cloud = useMemo(() => connectCloud(cloudSession(account)), [account]);
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

  const plan = PLANS.find((p) => p.id === church.plan);
  const planName = plan?.name ?? church.plan;
  const price = plan ? (church.billing === "yearly" ? plan.yearly : plan.monthly) : null;
  const per = church.billing === "yearly" ? "a year" : "a month";
  const founding = price !== null && FOUNDING_OFFER ? formatPrice(foundingPrice(price)) : null;
  const daysLeft = Math.ceil((church.trialEndsAt.getTime() - Date.now()) / DAY_MS);
  const trialOn = daysLeft > 0;
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
      <header className="page-header">
        <h1 className="page-title">Welcome, {account.name}</h1>
      </header>

      <section className="grouped" aria-labelledby="trial-heading">
        <div className="group">
          <div className="row">
            <span className="row-icon row-icon--accent">
              <CircleCheck aria-hidden="true" />
            </span>
            <span className="row-text">
              <span id="trial-heading">
                {trialOn
                  ? `${planName} trial · ${daysLeft} ${daysLeft === 1 ? "day" : "days"} left`
                  : `${planName} trial ended`}
              </span>
              <span className="text-caption">
                {price !== null ? `Then ${formatPrice(price)} ${per}` : "Then billed by plan"}
                {founding ? `, or ${founding} with the founding church offer` : ""}. Nothing is charged yet.
              </span>
            </span>
          </div>
        </div>
      </section>

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

          <Services cloud={cloud} />
        </div>

        <aside className="page-side">
          <section className="grouped" aria-labelledby="acct-heading">
            <div className="section-head">
              <h2 id="acct-heading">Account</h2>
            </div>
            <dl className="group">
              <div className="row">
                <dt className="row-text">Name</dt>
                <dd className="row-value">{account.name}</dd>
              </div>
              <div className="row">
                <dt className="row-text">Email</dt>
                <dd className="row-value account__email">{account.email}</dd>
              </div>
              <div className="row">
                <dt className="row-text">Church</dt>
                <dd className="row-value">{church.name}</dd>
              </div>
              <div className="row">
                <dt className="row-text">Role</dt>
                <dd className="row-value">{ROLE_LABEL[church.role]}</dd>
              </div>
              <div className="row">
                <dt className="row-text">Plan</dt>
                <dd className="row-value">
                  {planName}, {church.billing}
                  {trialOn ? " (trial)" : ""}
                </dd>
              </div>
            </dl>
          </section>
        </aside>
      </div>
    </>
  );
}

/** Services the church's booth computers have uploaded. */
function Services({ cloud }: { cloud: RecordingsCloud }) {
  const [list, setList] = useState<CloudRecording[] | null>(null);
  const [error, setError] = useState(false);

  useEffect(() => {
    cloud.listRecordings().then(setList, () => setError(true));
  }, [cloud]);

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
