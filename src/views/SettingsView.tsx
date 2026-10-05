import { Check, Lock } from "lucide-react";
import { useState } from "react";
import { PageHeader } from "../components/PageHeader";
import { getBackend } from "../lib/backend";
import { FEATURES, lockReason, PLAN_NAME, STATUS_LABEL } from "../lib/plans";
import type { Session } from "../lib/types";
import { useMixer, type Theme } from "../store/mixer";

const ROLE_LABEL = {
  admin: "Admin",
  engineer: "Engineer",
  volunteer: "Volunteer",
};
const THEMES: { value: Theme; label: string }[] = [
  { value: "dark", label: "Dark" },
  { value: "light", label: "Light" },
  { value: "system", label: "Match Mac" },
];

export function SettingsView() {
  const session = useMixer((s) => s.session);
  const theme = useMixer((s) => s.theme);
  const setTheme = useMixer((s) => s.setTheme);
  return (
    <div className="page page--narrow">
      <PageHeader title="Settings" />
      {session && <AccountSection session={session} />}
      {session && <PlanSection session={session} />}
      <section className="section" aria-labelledby="appearance-title">
        <div className="section-head">
          <h2 id="appearance-title">Appearance</h2>
        </div>
        <div className="group">
          <div className="row">
            <span className="row-text" id="theme-label">
              Theme
            </span>
            <div className="sm-seg" role="group" aria-labelledby="theme-label">
              {THEMES.map((t) => (
                <button key={t.value} aria-pressed={theme === t.value} onClick={() => setTheme(t.value)}>
                  {t.label}
                </button>
              ))}
            </div>
          </div>
        </div>
        <p className="section-foot">SanctuaryMix 0.1.0</p>
      </section>
    </div>
  );
}

function AccountSection({ session }: { session: Session }) {
  const signOut = useMixer((s) => s.signOut);
  const [error, setError] = useState<string | null>(null);
  // Signing in again just brings the sign-in screen back.
  const showSignIn = () => useMixer.setState({ workingLocally: false });
  const out = async () => {
    setError(null);
    try {
      await signOut();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <section className="section" aria-labelledby="account-title">
      <div className="section-head">
        <h2 id="account-title">Account</h2>
      </div>
      <div className="group">
        <div className="row">
          <div className="avatar" aria-hidden>
            {session.user.displayName.slice(0, 1)}
          </div>
          <span className="row-text">
            <span className="text-body-strong">{session.user.displayName}</span>
            <span className="text-caption">{session.user.email ?? "This computer only"}</span>
          </span>
          {session.authenticated ? (
            <button className="sm-btn" onClick={() => void out()}>
              Sign out
            </button>
          ) : (
            <button className="sm-btn sm-btn--primary" onClick={showSignIn}>
              Sign in
            </button>
          )}
        </div>
        {session.activeOrg && (
          <div className="row">
            <span className="row-text">Church</span>
            <span className="muted">{session.activeOrg.name}</span>
          </div>
        )}
        {session.authenticated && (
          <div className="row">
            <span className="row-text">Role</span>
            <span className="muted">{ROLE_LABEL[session.role]}</span>
          </div>
        )}
      </div>
      {error && <p className="error section-foot">{error}</p>}
    </section>
  );
}

function PlanSection({ session }: { session: Session }) {
  const { plan, status, entitlements } = session.access;
  const isAdmin = session.role === "admin";
  return (
    <section className="section" aria-labelledby="plan-title">
      <div className="section-head">
        <h2 id="plan-title">Plan</h2>
      </div>
      <div className="group">
        <div className="row">
          <span className="row-text">Plan</span>
          <span className="muted">
            {plan ? `${PLAN_NAME[plan]}${status ? ` · ${STATUS_LABEL[status]}` : ""}` : "None"}
          </span>
        </div>
        {plan && (
          <>
            <div className="row">
              <span className="row-text">Auto-mix channels</span>
              <span className="muted">
                {entitlements.maxAiChannels > 0 ? (
                  <>
                    Up to <span className="text-readout">{entitlements.maxAiChannels}</span>
                  </>
                ) : (
                  "None"
                )}
              </span>
            </div>
            <div className="row">
              <span className="row-text">Team logins</span>
              <span className="muted">{entitlements.teamLogins ?? "Unlimited"}</span>
            </div>
            <div className="row">
              <span className="row-text">Rooms</span>
              <span className="text-readout muted">{entitlements.rooms}</span>
            </div>
          </>
        )}
        {FEATURES.map(({ feature, label }) => {
          const lock = lockReason(session, feature);
          return (
            <div className={`row ${lock ? "row--locked" : ""}`} key={feature}>
              <span className="row-text">
                <span>{label}</span>
              </span>
              {lock ? (
                <span className="plan-lock">
                  <Lock aria-hidden strokeWidth={1.75} />
                  {lock}
                </span>
              ) : (
                <Check className="row-check" size={20} strokeWidth={1.75} aria-label="Included" />
              )}
            </div>
          );
        })}
        {plan && isAdmin && (
          <div className="row row--actions">
            <span className="muted">Billing is on the website.</span>
            <button className="sm-btn" onClick={() => void getBackend().then((b) => b.openBilling())}>
              Manage billing
            </button>
          </div>
        )}
      </div>
      <p className="section-foot">Console control, manual mixing and playback never need a plan.</p>
    </section>
  );
}
