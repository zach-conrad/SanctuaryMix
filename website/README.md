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

## Design

Uses the SanctuaryMix design system: `src/styles/tokens.css` and `components.css` are copies of `design/tokens.css` and `design/components.css`, fonts are bundled with `@fontsource`, and the logos in `public/brand` are the supplied wordmark files. Dark theme is the default (`data-theme` on `<html>`).

Hosting isn't decided yet. The build uses a relative base, so `dist/` works at a domain root or under a sub-path such as GitHub Pages.

## Pricing

Plans and prices live in one file, `src/pricing.ts`. The pricing section on the home page (monthly/yearly toggle, yearly saving) and the account page both read from it. While `PRICES_ARE_PLACEHOLDERS` is `true` the section says the prices are samples. `FOUNDING_OFFER` controls the founding-church banner; set it to `null` to remove it. Checkout will use Stripe.

There's no checkout yet. **Start free trial** opens the sample account page as `/account/?plan=pro&billing=yearly`, which shows the chosen plan; a real checkout will replace that link.
