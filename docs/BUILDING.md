# Building SanctuaryMix on a Mac

GitHub Actions is **paused** for now (since 2026-10-04). A private repo gets
2,000 free Actions minutes a month, macOS minutes count 10x, and building the
Mac app on every push used them all up. Instead, the app is built on a Mac,
by hand or by a Claude session running on that Mac.

## One-time setup

In Terminal:

```sh
xcode-select --install                 # Apple's command line tools (a dialog opens)
curl https://sh.rustup.rs -sSf | sh    # Rust; press Enter for the defaults, then open a new Terminal
brew install node@22                   # Node.js 22 (needs Homebrew: https://brew.sh)
```

Then get the code and install its packages:

```sh
git clone https://github.com/zach-conrad/SanctuaryMix.git
cd SanctuaryMix
npm install
```

The repo picks the exact Rust version itself (`rust-toolchain.toml`), and
`npm install` also sets up git hooks that run the checks before every push.

## Build the app

```sh
git pull                 # get the newest code
npm install              # only needed when packages changed; safe to run anyway
npm run tauri build
```

- The app: `target/release/bundle/macos/SanctuaryMix.app`
- The installer: `target/release/bundle/dmg/SanctuaryMix_<version>_<arch>.dmg`

Drag the app into Applications. It isn't signed yet, so the first time,
right-click it and choose **Open**. The first build takes several minutes while
Rust compiles everything; later builds are much faster.

For one `.dmg` that runs on both Apple Silicon and Intel Macs:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin   # once
npm run app:build:mac    # output in target/universal-apple-darwin/release/bundle/
```

To run every check CI would have run: `npm run check`.

## What is still on in GitHub Actions

| Workflow | Runs automatically? |
| --- | --- |
| CI (lint, tests, Mac and Windows builds) | No. Actions tab, CI, **Run workflow**. Also runs inside a release. |
| Security scans | No. Actions tab, Security, **Run workflow**. |
| Dev build (`dev` prerelease) | No. Actions tab, Dev build, **Run workflow**. |
| Release (`v*.*.*` tag) | Only when you push a version tag. Uses about 250 to 300 minutes. |
| Website deploy | Yes, when `website/` changes on main or after a release (a minute or two on Linux). |

Pull requests show no checks while this is paused, including Dependabot's.
The git hooks on whoever pushes still run lint, typecheck, tests and clippy.

## Turning automatic CI back on

When you're ready to spend minutes again (or upgrade the plan), restore these
triggers:

- `.github/workflows/ci.yml`: add `push: {branches: [main]}`, `pull_request:`
  and `merge_group:` under `on:`.
- `.github/workflows/security.yml`: the same three, plus
  `schedule: [{cron: "0 13 * * 1"}]` for the weekly scan.
- `.github/workflows/dev-build.yml`: add
  `workflow_run: {workflows: [CI], types: [completed], branches: [main]}`.

The macOS and Windows jobs in `ci.yml` stay manual-only even then; that alone
keeps a typical month inside the free minutes.
