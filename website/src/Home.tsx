import { Cable, Laptop, LogIn, Monitor, SlidersVertical, Sparkles, Users, WifiOff } from "lucide-react";
import type { ReactNode } from "react";
import { Footer } from "./components/Footer";
import { Header } from "./components/Header";
import { Pricing } from "./components/Pricing";

const ROOT = "./";

const FEATURES: { icon: ReactNode; title: string; body: string; assist?: boolean }[] = [
  {
    icon: <Cable aria-hidden="true" />,
    title: "Plugs into the console you have",
    body: "SanctuaryMix connects to your Allen & Heath dLive over the network and listens to every channel over Dante. Channel names, colors and mutes match the desk exactly.",
  },
  {
    icon: <Sparkles aria-hidden="true" />,
    title: "Suggestions you can check by ear",
    body: "Assist watches levels through the service and suggests small, specific moves, like raising the pastor's mic 2 dB. Nothing changes until you press Apply, and every change has Undo.",
    assist: true,
  },
  {
    icon: <Users aria-hidden="true" />,
    title: "Made for volunteers",
    body: "Big controls, plain words and a dark screen that reads from arm's length in the booth. A first-timer can follow along; an experienced tech can move fast.",
  },
  {
    icon: <WifiOff aria-hidden="true" />,
    title: "Works without internet",
    body: "Everything that touches the live mix runs on the booth computer. If the building's internet drops mid-service, the mix doesn't notice.",
  },
];

export function Home() {
  return (
    <>
      <Header root={ROOT} />
      <main>
        <section className="hero">
          <div className="site-container hero__inner">
            <p className="text-label hero__eyebrow">For church sound teams</p>
            <h1 className="hero__title">A steady Sunday mix, even with a new volunteer at the desk.</h1>
            <p className="hero__lede">
              SanctuaryMix sits beside your Allen &amp; Heath dLive and helps whoever is mixing hear what needs
              attention: a quiet pastor, a hot kick, a mic left open. It suggests the fix and you decide.
            </p>
            <div className="hero__actions">
              <a className="sm-btn sm-btn--primary sm-btn--lg" href="#pricing">
                See plans and pricing
              </a>
              <a className="sm-btn sm-btn--ghost sm-btn--lg" href={`${ROOT}account/`}>
                <LogIn aria-hidden="true" />
                Log in to download
              </a>
            </div>
            <p className="hero__note text-caption">Early access for macOS. Windows is planned.</p>
          </div>
          <div className="site-container">
            <figure className="hero__shot">
              <img
                src={`${ROOT}screenshots/sanctuarymix-mix.png`}
                alt="The SanctuaryMix mixer: twelve channel strips named Pastor, Worship Ld, BGV, Kick, Snare and more, with level meters, faders, mute keys and an assistant panel on the right."
                width={1440}
                height={900}
              />
            </figure>
          </div>
        </section>

        <section id="features" className="section">
          <div className="site-container">
            <h2 className="section__title">What it does</h2>
            <p className="section__lede">
              Your console stays in charge. SanctuaryMix gives the person behind it a second pair of ears.
            </p>
            <ul className="features">
              {FEATURES.map((f) => (
                <li key={f.title} className={`feature${f.assist ? " feature--assist" : ""}`}>
                  <span className="feature__icon">{f.icon}</span>
                  <h3 className="text-heading">{f.title}</h3>
                  <p className="feature__body">{f.body}</p>
                </li>
              ))}
            </ul>
          </div>
        </section>

        <section className="section section--surface">
          <div className="site-container steps">
            <div>
              <h2 className="section__title">Set up in one afternoon</h2>
              <ol className="steps__list">
                <li>
                  <strong>Install on the booth Mac.</strong> Download the app from your account and drag it to
                  Applications.
                </li>
                <li>
                  <strong>Connect to your dLive.</strong> Pick the console from the list. SanctuaryMix finds it on
                  the network.
                </li>
                <li>
                  <strong>Turn on Dante.</strong> Route the channels you want Assist to listen to, then check the
                  meters move.
                </li>
              </ol>
            </div>
            <figure className="steps__shot">
              <img
                src={`${ROOT}screenshots/sanctuarymix-setup.png`}
                alt="The SanctuaryMix setup screen for choosing a console and Dante device."
                loading="lazy"
                width={1440}
                height={900}
              />
            </figure>
          </div>
        </section>

        <section id="platforms" className="section">
          <div className="site-container">
            <h2 className="section__title">Mac first, Windows next</h2>
            <div className="platforms">
              <div className="platform">
                <Laptop aria-hidden="true" />
                <div>
                  <h3 className="text-heading">macOS</h3>
                  <p className="feature__body">Available now in early access for Apple silicon and Intel Macs.</p>
                </div>
                <span className="sm-pill sm-pill--ok">
                  <span className="sm-pill__dot" aria-hidden="true" />
                  Available
                </span>
              </div>
              <div className="platform">
                <Monitor aria-hidden="true" />
                <div>
                  <h3 className="text-heading">Windows</h3>
                  <p className="feature__body">The app is built to run on Windows too. We'll release it after the Mac version settles.</p>
                </div>
                <span className="sm-pill">
                  <span className="sm-pill__dot" aria-hidden="true" />
                  Planned
                </span>
              </div>
              <div className="platform">
                <SlidersVertical aria-hidden="true" />
                <div>
                  <h3 className="text-heading">More consoles</h3>
                  <p className="feature__body">We're starting with the Allen &amp; Heath dLive. Other desks will follow.</p>
                </div>
                <span className="sm-pill">
                  <span className="sm-pill__dot" aria-hidden="true" />
                  Planned
                </span>
              </div>
            </div>
          </div>
        </section>

        <Pricing root={ROOT} />

        <section className="section cta">
          <div className="site-container cta__inner">
            <h2 className="section__title">Ready to try it in your booth?</h2>
            <div className="cta__actions">
              <a className="sm-btn sm-btn--ghost sm-btn--lg" href={`${ROOT}account/`}>
                <LogIn aria-hidden="true" />
                Log in
              </a>
              <a className="sm-btn sm-btn--primary sm-btn--lg" href="#pricing">
                Choose a plan
              </a>
            </div>
          </div>
        </section>
      </main>
      <Footer />
    </>
  );
}
