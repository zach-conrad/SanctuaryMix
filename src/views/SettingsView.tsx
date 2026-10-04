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
      <section className="section" aria-labelledby="account-title">
        <div className="section-head">
          <h2 id="account-title">Account</h2>
        </div>
        {session && (
          <div className="group">
            <div className="row">
              <div className="avatar" aria-hidden>
                {session.user.displayName.slice(0, 1)}
              </div>
              <span className="row-text">
                <span className="text-body-strong">{session.user.displayName}</span>
                <span className="text-caption">
                  {ROLE_LABEL[session.role]} · {session.activeOrg?.name ?? "This computer"}
                </span>
              </span>
              <button className="sm-btn" disabled title="Coming soon">
                Sign in
              </button>
            </div>
          </div>
        )}
      </section>
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
