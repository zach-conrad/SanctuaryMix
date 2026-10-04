# SanctuaryMix website

A preliminary launch site: a landing page and a sample account page with the Mac download. There is no real sign-in yet; **Log in** goes straight to the sample account at `/account/`.

```sh
cd website
npm install
npm run dev      # http://localhost:5173
npm run build    # static output in website/dist
```

## Download link

The Download button is fed by CI. Each release the pipeline publishes a GitHub Release on `zach-conrad/SanctuaryMix` with the Mac installer under a stable asset name, `SanctuaryMix-macOS-universal.dmg`. The site links to

```
https://github.com/zach-conrad/SanctuaryMix/releases/latest/download/SanctuaryMix-macOS-universal.dmg
```

which GitHub redirects to the newest published release, so the site doesn't need a rebuild when a new build ships. The account page also asks the GitHub API for the latest version, date and size, and falls back to "Latest build" if it can't reach it.

While the repo is private, that link only works for people signed in to GitHub with access to the repo. To change where downloads come from, edit `src/release.ts` only.

## Design

Uses the SanctuaryMix design system: `src/styles/tokens.css` and `components.css` are copies of `design/tokens.css` and `design/components.css`, fonts are bundled with `@fontsource`, and the logos in `public/brand` are the supplied wordmark files. Dark theme is the default (`data-theme` on `<html>`).

Hosting isn't decided yet. The build uses a relative base, so `dist/` works at a domain root or under a sub-path such as GitHub Pages.
