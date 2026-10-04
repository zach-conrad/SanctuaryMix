# CI/CD and release flow

The goal: catch mistakes at the earliest, cheapest point, and make it
impossible for anything to reach users without passing every check and going
through the dev channel first.

```
 your machine            pull request               main                    tag vX.Y.Z
 ────────────            ────────────               ────                    ──────────
 pre-commit hook   ──▶   CI: fast checks      ──▶   CI again           ──▶  CI again on the tag
 (format, lint,          then Mac + Windows         Dev build ──▶           wait for approval
  secret scan)           builds, Security           rolling "dev"           ("production" env)
 pre-push hook           scans, Mac preview          prerelease             sign, build, publish
 (= CI fast checks)      build to download                                  GitHub release,
                                                                                then deploy the
                                                                                website with it
```

## 1. On your machine (git hooks)

`npm install` installs the hooks through [lefthook](https://lefthook.dev)
(`lefthook.yml`). Re-install with `npm run hooks:install`.

| Hook | What runs | Time |
| --- | --- | --- |
| pre-commit | `rustfmt` and `eslint --fix` on staged files (fixes are re-staged), `gitleaks` secret scan of the staged diff | ~1-2 s |
| pre-push | `npm run check:web` (lint, typecheck, unit tests) and `npm run check:rust` (fmt, clippy `-D warnings`, tests), in parallel | under a minute once Rust is warm |

Install gitleaks once for the secret scan: `brew install gitleaks`. The hook
skips with a message if it is missing; CI still scans.

Run everything CI's fast checks run with `npm run check`. In an emergency,
`--no-verify` skips the hooks; CI still blocks the merge.

## 2. On every pull request

**CI** (`.github/workflows/ci.yml`)

1. Fast checks, in parallel: **Web** (ESLint, `tsc`, Vitest, Vite build) and
   **Rust** (rustfmt, clippy with warnings as errors, tests, `--locked` so
   `Cargo.lock` must be up to date).
2. Only if those pass: **macOS** universal `.app` + `.dmg` build and a
   **Windows** clippy + test run (catches `cfg(windows)` code). This saves
   macOS minutes, which GitHub bills at 10x on private repos.
3. **CI passed**: one summary check. This is the one to require in branch
   protection.

The Mac build on a PR is the **preview build**: open the PR's CI run, then
Summary, then download `sanctuarymix-macos-prN`. Kept for 14 days.

**Security** (`.github/workflows/security.yml`), also weekly on Mondays:

- **gitleaks** scans the full git history for keys and tokens.
- **cargo-deny** (`deny.toml`): RustSec advisories, license allowlist,
  banned/duplicate crates, crates only from crates.io. A newly published
  advisory is reported but doesn't block unrelated PRs; the weekly run and
  Dependabot handle it.
- **npm audit**: fails on high/critical issues in shipped dependencies.

**Dependabot** (`.github/dependabot.yml`) opens grouped weekly update PRs for
Cargo, npm and GitHub Actions. Each runs full CI.

**Pinned Rust**: `rust-toolchain.toml` pins an exact Rust version so a new
Rust release can't turn CI red overnight (that's what broke the first PR).
To upgrade, change the version in one PR and fix any new clippy lints there.

## 3. Dev channel (every merge to main)

`.github/workflows/dev-build.yml` runs after CI passes on `main`. It builds
the Mac `.dmg` and Windows installer using the **development** environment's
settings and replaces the rolling **`dev`** prerelease:

- `https://github.com/zach-conrad/SanctuaryMix/releases/tag/dev`
- Files: `SanctuaryMix-dev-mac-universal.dmg`, `SanctuaryMix-dev-windows-x64-setup.exe`

This is where a change gets tried on real booth hardware before it is
released. When the Supabase backend arrives, the development environment
points at a separate dev Supabase project (see section 6).

## 4. Production release (tag)

1. Bump the version in `package.json`, `src-tauri/tauri.conf.json` and
   `Cargo.toml` (`[workspace.package] version`), merge that to `main`.
2. Tag the merged commit and push the tag:
   `git tag v0.2.0 && git push origin v0.2.0`
3. `.github/workflows/release.yml` then:
   - checks the three versions match the tag and the commit is on `main`;
   - re-runs all of CI on that exact commit;
   - **waits for approval** on the `production` environment (once reviewers
     are set, see below);
   - builds the Mac universal `.dmg` (signed and notarized once Apple secrets
     exist) and Windows `.exe`/`.msi`;
   - publishes a GitHub release (private, like the repo) with generated notes
     and `SHA256SUMS.txt`;
   - triggers the website deploy, which puts the new installer on the site
     (see "Website downloads" below).

Assets use fixed names (`SanctuaryMix-mac-universal.dmg`,
`SanctuaryMix-windows-x64-setup.exe`) so "latest" paths never change.

## 5. One-time setup for Zach

None of this is done automatically. Everything works without it, just with
fewer guarantees.

### Branch protection for `main` (most important)

Settings, then Rules, then Rulesets, then New branch ruleset, target `main`:

- Require a pull request before merging (1 approval if anyone else
  contributes; with Code Owners review on so pipeline changes need you).
- Require status checks to pass: add **CI passed**, **Secret scan
  (gitleaks)**, **Rust deps (bans licenses sources)** and **npm audit**.
  Turn on "Require branches to be up to date".
- Block force pushes and deletions.
- Optional: require linear history.

### Environments

Settings, then Environments:

- **production**: add yourself as a required reviewer, and restrict
  deployment to tags matching `v*.*.*`. This is the approval gate before
  anything ships.
- **development**: no reviewers needed. Restrict to the `main` branch.

### Security features (free on private repos)

Settings, then Code security: turn on Dependabot alerts, Dependabot security
updates, and (if offered) Secret Protection push protection.

CodeQL code scanning and the dependency-review action need GitHub Advanced
Security on private repos, so they are not included. They can be added if
the repo becomes public or GHAS is enabled.

### Secrets and variables (when ready)

Put these on the **production** environment, not the repo:

| Name | Type | For |
| --- | --- | --- |
| `APPLE_CERTIFICATE` | secret | Developer ID Application cert, `.p12` base64 encoded |
| `APPLE_CERTIFICATE_PASSWORD` | secret | Password for that `.p12` |
| `APPLE_SIGNING_IDENTITY` | secret | e.g. `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_ID` | secret | Apple ID email used for notarization |
| `APPLE_PASSWORD` | secret | App-specific password for that Apple ID |
| `APPLE_TEAM_ID` | secret | 10-character team ID |

Signing needs an Apple Developer Program membership ($99/year).

### Website downloads (Netlify)

End users download from the website, never from GitHub. GitHub releases stay
private as the internal copy. `.github/workflows/website.yml` builds
`website/`, copies the newest release's `SanctuaryMix-mac-universal.dmg` into
`downloads/` with a `latest.json` manifest (contract: `website/src/release.ts`),
and deploys to Netlify. It runs after every release, on every change to
`website/` on main, and by hand. CI also builds the site on every PR.

To turn deploys on:

1. Create a free Netlify account and a site (Add new site, then Deploy
   manually; any placeholder folder is fine, CI replaces it).
2. Add two **repository** secrets (Settings, then Secrets and variables, then
   Actions):

| Secret | Value |
| --- | --- |
| `NETLIFY_AUTH_TOKEN` | Netlify, then User settings, then Applications, then Personal access token |
| `NETLIFY_SITE_ID` | Netlify, then Site configuration, then Site ID |

Until real sign-in exists, anyone with the site's address can download. The
installer is not linked from GitHub or listed anywhere public. The planned
upgrade is to store installers in a private Supabase Storage bucket and have
the account page hand signed-in users short-lived signed links.

## 6. Backend environments (later)

When Supabase sync lands, create two Supabase projects, one for dev and one
for production, and set environment **variables** on each GitHub
environment:

| Variable | development | production |
| --- | --- | --- |
| `SUPABASE_URL` | dev project URL | prod project URL |
| `SUPABASE_ANON_KEY` | dev anon key | prod anon key |

The dev and release workflows already pass the URL and anon key to the build as
`VITE_SUPABASE_URL` / `VITE_SUPABASE_ANON_KEY`, plus `VITE_APP_CHANNEL`
(`dev` or `stable`). The anon key is public by design; never put the
Supabase service-role key in the app or in these workflows. Database
migrations would get their own workflow that applies to dev on merge and to
prod on release, behind the same approval.
