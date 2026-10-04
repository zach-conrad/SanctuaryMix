# SanctuaryMix website

A preliminary launch site: a landing page and a sample account page with the Mac download. There is no real sign-in yet; **Log in** goes straight to the sample account at `/account/`.

```sh
cd website
npm install
npm run dev      # http://localhost:5173
npm run build    # static output in website/dist
```

## Download link

Downloads are served by the website itself, so visitors never need a GitHub account. The CI/CD release workflow keeps them current: on each tagged release it copies the installer into the built site and writes a small manifest, then deploys the site.

```
dist/downloads/SanctuaryMix-mac-universal.dmg
dist/downloads/latest.json
  { "version": "0.2.0",
    "publishedAt": "2026-10-04T19:00:00Z",
    "mac": { "file": "SanctuaryMix-mac-universal.dmg", "size": 48213504 } }
```

The account page reads `downloads/latest.json` and points the Download button at the file it names. Until a release has been deployed there's no manifest, and the button shows as coming soon. A `windows` entry with the same shape is added once a Windows build ships. The contract lives in `src/release.ts`.

To try it locally, drop any file and a matching `latest.json` into `public/downloads/` (git-ignored) and run `npm run dev`.

Note: there's no real sign-in yet, so anyone who has the site's address can reach the account page and download. Hosting must allow files the size of the dmg (GitHub Pages caps files at 100 MB; Cloudflare Pages at 25 MB).

## Recorded services and share links

The account page lists the church's recorded services. Each opens at `/account/service/?id=<id>` with a player, the fader and mute timeline, the console state at the playhead, and the change list. **Share** makes a link to `/share/?t=<token>`, a public page that plays the mix (and the moves, if the link includes them) without signing in.

Pages talk to the cloud only through `RecordingsCloud` in `src/cloud/types.ts`. Today `DemoCloud` serves the sample service from `public/demo-cloud/org/<org_id>/recordings/<id>/` (the same keys as Supabase Storage: `manifest.json`, `events.jsonl`, and `listen.mp3`, a 2.4 MB MP3 of the sample mix) and keeps share links in the browser's storage. A share link opened in another browser plays the sample service with a preview note. A Supabase implementation replaces `DemoCloud` in `src/cloud/index.ts`; its draft schema is in `docs/supabase/share_links.sql`.

The timeline and recording helpers are shared with the desktop app: `src/lib/recordings.ts`, `levels.ts` and `types.ts` are imported from the app, and `src/mix/Timeline.tsx` is a copy of the app's component.

## Design

Uses the SanctuaryMix design system: `src/styles/tokens.css` and `components.css` are copies of `design/tokens.css` and `design/components.css`, fonts are bundled with `@fontsource`, and the logos in `public/brand` are the supplied wordmark files. Dark theme is the default (`data-theme` on `<html>`).

Hosting isn't decided yet. The build uses a relative base, so `dist/` works at a domain root or under a sub-path such as GitHub Pages.

## Pricing

Plans and prices live in one file, `src/pricing.ts`. The pricing section on the home page (monthly/yearly toggle, yearly saving) and the account page both read from it. While `PRICES_ARE_PLACEHOLDERS` is `true` the section says the prices are samples. `FOUNDING_OFFER` controls the founding-church banner; set it to `null` to remove it. Checkout will use Stripe.

There's no checkout yet. **Start free trial** opens the sample account page as `/account/?plan=pro&billing=yearly`, which shows the chosen plan; a real checkout will replace that link.
