# SanctuaryMix website

A preliminary launch site: a landing page and a sample account page with the Mac download. There is no real sign-in yet; **Log in** goes straight to the sample account at `/account/`.

```sh
cd website
npm install
npm run dev      # http://localhost:5173
npm run build    # static output in website/dist
```

## Download link

The Download button is fed by CI, and downloads stay private. Each tagged release, the CI/CD workflow publishes a GitHub Release on this private repo with the Mac installer under a stable asset name, `SanctuaryMix-mac-universal.dmg` (Windows later: `SanctuaryMix-windows-x64-setup.exe`). The site links to

```
https://github.com/zach-conrad/SanctuaryMix/releases/latest/download/SanctuaryMix-mac-universal.dmg
```

which GitHub redirects to the newest published release, so the site doesn't need a rebuild when a new build ships. Because the repo is private, the link only works for people signed in to GitHub with access to it; everyone else gets a 404. That is intended for now. When real sign-in exists, the account page should hand out a short-lived signed download link instead (for example from a private Supabase Storage bucket that CI uploads to).

The version, date and size lookup through the GitHub API is switched off (`RELEASES_ARE_PUBLIC = false`) because the API can't see private releases without a token.

To change where downloads come from, edit `src/release.ts` only.

## Design

Uses the SanctuaryMix design system: `src/styles/tokens.css` and `components.css` are copies of `design/tokens.css` and `design/components.css`, fonts are bundled with `@fontsource`, and the logos in `public/brand` are the supplied wordmark files. Dark theme is the default (`data-theme` on `<html>`).

Hosting isn't decided yet. The build uses a relative base, so `dist/` works at a domain root or under a sub-path such as GitHub Pages.
