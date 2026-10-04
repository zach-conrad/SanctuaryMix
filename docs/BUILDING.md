# Building SanctuaryMix on a Mac

CI builds the app on every pull request (see [CI.md](CI.md)), but you can
always build it yourself on a Mac, with no GitHub involved.

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
- The installer: `target/release/bundle/dmg/`

Drag the app into Applications. It isn't signed yet, so the first time,
right-click it and choose **Open**. The first build takes several minutes while
Rust compiles everything; later builds are much faster.

For one `.dmg` that runs on both Apple Silicon and Intel Macs (what CI builds):

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin   # once
npm run app:build:mac    # output in target/universal-apple-darwin/release/bundle/
```

To run every check CI's fast jobs run: `npm run check`.
