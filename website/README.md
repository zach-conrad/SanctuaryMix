# SanctuaryMix website

A preliminary launch site: a landing page and a sample account page with the Mac download. There is no real sign-in yet; **Log in** goes straight to the sample account at `/account/`.

```sh
cd website
npm install
npm run dev      # http://localhost:5173
npm run build    # static output in website/dist
```

## Download link

The Download button is fed by CI. The source repo is private, so the release workflow mirrors each tagged release to the public, binaries-only repo `zach-conrad/sanctuarymix-releases`, with the Mac installer under a stable asset name, `SanctuaryMix-mac-universal.dmg` (Windows later: `SanctuaryMix-windows-x64-setup.exe`). The site links to

```
https://github.com/zach-conrad/sanctuarymix-releases/releases/latest/download/SanctuaryMix-mac-universal.dmg
```

which GitHub redirects to the newest published release, so the site doesn't need a rebuild when a new build ships. The account page also asks the GitHub API for the latest version, date and size. Until the releases repo has its first release, the button shows as disabled with a "coming soon" note; if the API can't be reached, the button stays on and the details read "Latest build".

To change where downloads come from, edit `src/release.ts` only.

## Design

Uses the SanctuaryMix design system: `src/styles/tokens.css` and `components.css` are copies of `design/tokens.css` and `design/components.css`, fonts are bundled with `@fontsource`, and the logos in `public/brand` are the supplied wordmark files. Dark theme is the default (`data-theme` on `<html>`).

Hosting isn't decided yet. The build uses a relative base, so `dist/` works at a domain root or under a sub-path such as GitHub Pages.
