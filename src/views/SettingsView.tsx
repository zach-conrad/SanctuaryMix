import { CircleUser } from "lucide-react";
import { useMixer, type Theme } from "../store/mixer";

const ROLE_LABEL = { admin: "Admin", engineer: "Engineer", volunteer: "Volunteer" };
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
    <div className="page setup">
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
            <div>
              <div className="text-body-strong">{session.user.displayName}</div>
              <div className="text-caption muted">
                {ROLE_LABEL[session.role]} · {session.activeOrg?.name ?? "This computer"} ·{" "}
                {session.authenticated ? session.user.email : "Not signed in"}
              </div>
            </div>
          </div>
        )}
        <p className="muted">
          Team accounts will let your church share scenes, Assist settings and service history across computers. Until
          then, SanctuaryMix runs on this computer with full control and no sign-in.
        </p>
        <div className="actions">
          <button className="sm-btn sm-btn--lg" disabled title="Coming soon">
            Sign in
          </button>
        </div>
      </section>
      <section className="panel">
        <h2 className="text-heading">Appearance</h2>
        <p className="muted">Dark is easiest on the eyes in a dark room. Use Light for daytime setup.</p>
        <div className="sm-seg" role="group" aria-label="Theme">
          {THEMES.map((t) => (
            <button key={t.value} aria-pressed={theme === t.value} onClick={() => setTheme(t.value)}>
              {t.label}
            </button>
          ))}
        </div>
        <p className="text-caption muted">SanctuaryMix 0.1.0</p>
      </section>
    </div>
  );
}
