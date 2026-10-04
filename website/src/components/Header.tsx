import { LogIn, LogOut } from "lucide-react";
import { Wordmark } from "./Wordmark";

interface Props {
  /** Relative path back to the site root from the current page. */
  root: string;
  signedIn?: boolean;
}

export function Header({ root, signedIn = false }: Props) {
  return (
    <header className="site-header">
      <div className="site-container site-header__inner">
        <a href={root} className="site-header__home" aria-label="SanctuaryMix home">
          <Wordmark root={root} />
        </a>
        {signedIn ? (
          <nav className="site-header__nav" aria-label="Account">
            <span className="site-header__user text-caption">Alex Rivera · Grace Community Church</span>
            <a className="sm-btn sm-btn--ghost" href={root}>
              <LogOut aria-hidden="true" />
              Sign out
            </a>
          </nav>
        ) : (
          <nav className="site-header__nav" aria-label="Main">
            <a className="site-header__link" href={`${root}#features`}>Features</a>
            <a className="site-header__link" href={`${root}#platforms`}>Mac and Windows</a>
            <a className="site-header__link" href={`${root}#pricing`}>Pricing</a>
            <a className="sm-btn" href={`${root}account/`}>
              <LogIn aria-hidden="true" />
              Log in
            </a>
          </nav>
        )}
      </div>
    </header>
  );
}
