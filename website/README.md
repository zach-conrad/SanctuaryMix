# SanctuaryMix website

A preliminary launch site: a landing page and a sample account page with the Mac download. There is no real sign-in yet; **Log in** goes straight to the sample account at `/account/`.

```sh
cd website
npm install
npm run dev      # http://localhost:5173
npm run build    # static output in website/dist
```

## Download link

Visitors never need a GitHub account, and there is no permanent public link to the installer.

- The CI/CD release workflow uploads every tagged release to a **private** Supabase Storage bucket, `releases`, at `stable/latest/SanctuaryMix-mac-universal.dmg` (Windows later: `SanctuaryMix-windows-x64-setup.exe`). It can also write `stable/latest/latest.json` with `{ "version", "publishedAt" }`.
- The `download` Edge Function (`supabase/functions/download`) finds that file and returns its version, date, size and a signed link that expires after 5 minutes.
- The account page calls the function on load to show the version, and again when someone clicks **Download for Mac**, so the link is always fresh.

Because the newest build is read at click time, the website never needs a redeploy for a new release.

Build settings, from `website/.env` or CI:

```
VITE_SUPABASE_URL=https://<project>.supabase.co
VITE_SUPABASE_ANON_KEY=<anon key>   # optional while verify_jwt is off
```

Without `VITE_SUPABASE_URL` the button shows as coming soon. Deploy the function with `supabase functions deploy download` from the repo root.

Sign-in is a sample for now, so the function doesn't check who is asking (`verify_jwt = false` in `supabase/config.toml`). When real Supabase Auth lands, turn that on and check church membership in the function before signing.

## Design

Uses the SanctuaryMix design system: `src/styles/tokens.css` and `components.css` are copies of `design/tokens.css` and `design/components.css`, fonts are bundled with `@fontsource`, and the logos in `public/brand` are the supplied wordmark files. Dark theme is the default (`data-theme` on `<html>`).

Hosting isn't decided yet. The build uses a relative base, so `dist/` works at a domain root or under a sub-path such as GitHub Pages.
