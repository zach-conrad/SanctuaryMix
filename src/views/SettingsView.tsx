import { CircleUser } from "lucide-react";
import { PageHeader } from "../components/PageHeader";
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
      <section className="panel">
        <div className="panel-head">
          <CircleUser size={20} strokeWidth={1.75} />
          <h2 className="text-heading">Account</h2>
        </div>
        {session && (
          <div className="account">
            <div className="avatar" aria-hidden>
              {session.user.displayName.slice(0, 1)}
            </div>
            <div className="setting-row-text">
              <div className="text-body-strong">{session.user.displayName}</div>
              <div className="text-caption muted">
                {ROLE_LABEL[session.role]} · {session.activeOrg?.name ?? "This computer"} ·{" "}
                {session.authenticated ? session.user.email : "Not signed in"}
              </div>
            </div>
            <button className="sm-btn" disabled title="Coming soon">
              Sign in
            </button>
          </div>
        )}
      </section>
      <section className="panel">
        <h2 className="text-heading">Appearance</h2>
        <div className="setting-row">
          <div className="setting-row-text">
            <span className="text-body-strong" id="theme-label">
              Theme
            </span>
          </div>
          <div className="sm-seg" role="group" aria-labelledby="theme-label">
            {THEMES.map((t) => (
              <button key={t.value} aria-pressed={theme === t.value} onClick={() => setTheme(t.value)}>
                {t.label}
              </button>
            ))}
          </div>
        </div>
        <p className="text-caption muted">SanctuaryMix 0.1.0</p>
      </section>
    </div>
  );
}
