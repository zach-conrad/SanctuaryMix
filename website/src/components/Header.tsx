import { LogIn, LogOut } from "lucide-react";
import { Wordmark } from "./Wordmark";

interface Props {
  /** Relative path back to the site root from the current page. */
  root: string;
  /** Who is signed in, on account pages. Leave out for the public header. */
  user?: { name: string; church: string } | null;
  onSignOut?: () => void;
  /** Account pages show no marketing links, even while checking who is signed in. */
  account?: boolean;
}

export function Header({ root, user, onSignOut, account = false }: Props) {
  return (
    <header className="site-header">
      <div className="site-container site-header__inner">
        <a href={root} className="site-header__home" aria-label="SanctuaryMix home">
          <Wordmark root={root} />
        </a>
        {user ? (
          <nav className="site-header__nav" aria-label="Account">
            <span className="site-header__user text-caption">
              {user.name} · {user.church}
            </span>
            <button type="button" className="sm-btn sm-btn--ghost" onClick={onSignOut}>
              <LogOut aria-hidden="true" />
              Sign out
            </button>
          </nav>
        ) : account ? null : (
          <nav className="site-header__nav" aria-label="Main">
            <a className="site-header__link" href={`${root}#features`}>Features</a>
            <a className="site-header__link" href={`${root}#platforms`}>Mac and Windows</a>
            <a className="site-header__link" href={`${root}#pricing`}>Pricing</a>
            <a className="sm-btn" href={`${root}account/`}>
              <LogIn aria-hidden="true" />
              Sign in
            </a>
          </nav>
        )}
      </div>
    </header>
  );
}
