import { CircleUser } from "lucide-react";
import { useMixer } from "../store/mixer";

const ROLE_LABEL = { admin: "Admin", engineer: "Engineer", volunteer: "Volunteer" };

export function SettingsView() {
  const session = useMixer((s) => s.session);
  return (
    <div className="page setup">
      <section className="card">
        <div className="card-head">
          <CircleUser size={20} />
          <h2>Account</h2>
        </div>
        {session && (
          <div className="account">
            <div className="avatar">{session.user.displayName.slice(0, 1)}</div>
            <div>
              <strong>{session.user.displayName}</strong>
              <span className="muted">
                {ROLE_LABEL[session.role]} · {session.activeOrg?.name ?? "This computer"} ·{" "}
                {session.authenticated ? session.user.email : "Not signed in"}
              </span>
            </div>
          </div>
        )}
        <p className="muted">
          Team accounts will let your tech team share saved scenes, AI preferences and service history across
          computers. Until then, SanctuaryMix runs as a local operator with full control.
        </p>
        <div className="actions">
          <button className="primary" disabled title="Coming soon">
            Sign in
          </button>
        </div>
      </section>
      <section className="card">
        <h2>About</h2>
        <p className="muted">SanctuaryMix 0.1.0. Built with Tauri, Rust and React.</p>
      </section>
    </div>
  );
}
