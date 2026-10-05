# SanctuaryMix website

A preliminary launch site: a landing page and an account page with the Mac download. Accounts are real: `/account/` signs people in with email and password or Google through Supabase Auth, and shows their church, role and trial. See [Accounts](#accounts).

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

Note: the Download button is on the signed-in account page, but the file itself is a static asset, so anyone with its address can download it. Hosting must allow files the size of the dmg (GitHub Pages caps files at 100 MB; Cloudflare Pages at 25 MB).

## Recorded services and share links

The account page lists the church's recorded services. Each opens at `/account/service/?id=<id>` with a player, the fader and mute timeline, the console state at the playhead, and the change list. **Share** makes a link to `/share/?t=<token>`, a public page that plays the mix (and the moves, if the link includes them) without signing in.

Pages talk to the cloud only through `RecordingsCloud` in `src/cloud/types.ts`. Signed-in pages use `AccountCloud` (`src/cloud/account.ts`), which knows who is signed in but has no services yet: recordings don't sync from the app until app sign-in is built, so the list shows its empty state. The public share page still uses `DemoCloud`, which serves the sample service from `public/demo-cloud/org/<org_id>/recordings/<id>/` (the same keys as Supabase Storage: `manifest.json`, `events.jsonl`, and `listen.mp3`, a 2.4 MB MP3 of the sample mix) and keeps share links in the browser's storage. A share link opened in another browser plays the sample service with a preview note. When recordings sync, `AccountCloud` reads them from Supabase and the share page switches over in `src/cloud/index.ts`; the draft schema is in `docs/supabase/recordings.sql` and `share_links.sql`.

The timeline and recording helpers are shared with the desktop app: `src/lib/recordings.ts`, `levels.ts` and `types.ts` are imported from the app, and `src/mix/Timeline.tsx` is a copy of the app's component.

## Accounts

`src/auth` holds the sign-in pieces: `client.ts` (the Supabase client, PKCE flow), `useAccount.ts` (who is signed in and their church) and `AuthForms.tsx` (sign in, create account, forgot password, name your church, set a new password).

- **Create account** asks for name, church, email and password (10+ characters with letters and numbers). Supabase emails a confirmation link back to `/account/`; opening it signs the person in and makes their church, with them as Admin, on a 30-day trial of the plan picked on the pricing section (`?plan=&billing=`, Pro if none).
- **Continue with Google** returns to `/account/`; a first-time Google user names their church there.
- **Forgot password** emails a link to `/account/?reset=1`, which asks for a new password.
- `/account/service/` asks people to sign in first.

The URL and publishable key in `client.ts` are public by design; `VITE_SUPABASE_URL` and `VITE_SUPABASE_PUBLISHABLE_KEY` override them at build time. Never put a secret or `service_role` key in the website. Tables, row-level security and the one-time Supabase dashboard setup are in [`supabase/README.md`](../supabase/README.md).

## Design

Uses the SanctuaryMix design system: `src/styles/tokens.css` and `components.css` are copies of `design/tokens.css` and `design/components.css`, fonts are bundled with `@fontsource`, and the logos in `public/brand` are the supplied wordmark files. Dark theme is the default (`data-theme` on `<html>`). Pages follow the app's Apple-style rules: panels separated by fill rather than outlines, settings and records as grouped lists (`.grouped` > `.section-head` > `.group` > `.row`, one group per service date), one footnote line per group, and centered empty states. App screenshots in `public/screenshots` come in `-dark` and `-light` pairs and follow the page theme.

Hosting isn't decided yet. The build uses a relative base, so `dist/` works at a domain root or under a sub-path such as GitHub Pages.

## Pricing

Plans and prices live in one file, `src/pricing.ts`. The pricing section on the home page (monthly/yearly toggle, yearly saving) and the account page both read from it. While `PRICES_ARE_PLACEHOLDERS` is `true` the section says the prices are samples. `FOUNDING_OFFER` controls the founding-church banner; set it to `null` to remove it. Checkout will use Stripe.

There's no checkout yet. **Start free trial** opens `/account/?plan=pro&billing=yearly`, which signs the person up and starts the trial on that plan; Stripe checkout will come later.
