# SanctuaryMix

AI-assisted live mixing for church services. SanctuaryMix listens to every channel
of your console over **Dante**, talks to the console over the network, and
(soon) helps volunteers and engineers with gain staging, feedback, EQ and
speech-to-music balance.

**Status:** UI/UX shell plus the first auto-mix: pick channels and a room
feel on the Assist screen and SanctuaryMix rides those faders inside hard
guardrails (see [docs/AUTOMIX.md](docs/AUTOMIX.md)). Not yet tried on a real
dLive.

- Mac first, packaged as a normal `SanctuaryMix.app` you drag into Applications.
- Windows builds from the same code (CI already checks it).
- Consoles: **Allen & Heath dLive** first. Others plug in behind one interface.
- Accounts: the interface is in place; sign-in comes later.

## Screens

| Screen | What it does |
| --- | --- |
| **Mixer** | One strip per input: live Dante meter, console fader and mute, channel name pulled from the desk. The inspector shows the selected channel and Assist notes (clipping, hot and silent channels). |
| **Scenes** | Placeholder for scene recall. |
| **Assist** | Auto-mix: pick channels and how the room should feel, start, freeze, undo, and see every move with its reason. Plus live observations and what's coming (gain staging, feedback watch, EQ). |
| **Setup** | Pick the audio input (Dante Virtual Soundcard is promoted to the top) and connect to the console. |
| **Settings** | Account placeholder for future team sign-in, and the theme (Dark, Light, Match Mac). |

Run `npm run dev` and open http://localhost:1420 to see the whole UI in a
browser with simulated audio and a simulated console.

## How it works

```
 dLive MixRack ──Dante audio──▶ Dante Virtual Soundcard ──CoreAudio──▶ audio-engine ──meters──┐
      ▲                                                                                       ▼
      └────────── TCP 51328 (MIDI protocol) ◀── console::DliveAdapter ◀── Tauri commands ◀── React UI
```

- **Audio** arrives over Dante. On a Mac, Dante Virtual Soundcard (or a Dante
  interface) shows up as a normal CoreAudio device with up to 64 inputs. We
  open every input, meter it on a lock-free real-time path, and publish levels
  to the UI ~30 times a second. The future AI analysis taps the same stream.
- **Control** goes straight to the console over TCP using Allen & Heath's
  published dLive MIDI protocol (faders, mutes, channel names). No A&H driver
  or MIDI app is needed.

## Why this stack

| Layer | Choice | Why |
| --- | --- | --- |
| App shell | [Tauri 2](https://tauri.app) | Real `.app`/`.dmg` on macOS and `.msi`/`.exe` on Windows from one codebase. ~10 MB instead of Electron's ~150 MB, and the native core is Rust. |
| Audio + DSP | Rust with [`cpal`](https://github.com/RustAudio/cpal) | No garbage collector, so no dropouts. CoreAudio on Mac and WASAPI/ASIO on Windows behind one API. The same crates can later run ML models (ONNX/Core ML) for the AI features. |
| UI | React + TypeScript + Vite | The most common, best-supported way to build a polished UI quickly. Hot reload while designing. |
| UI state | [Zustand](https://github.com/pmndrs/zustand) | Tiny and simple. Meters write CSS variables per animation frame, so 30 fps updates don't re-render the app. |
| Design | SanctuaryMix design system | `src/styles/tokens.css` and `components.css` are copies of the project's design tokens and `sm-` component styles. Figtree and IBM Plex Mono are bundled with `@fontsource`, so the app needs no internet. Dark theme by default. |

Native Swift was the alternative. It would give a slightly more "Mac" feel but
means writing the Windows app a second time, so it lost.

## Repository layout

```
crates/
  mix-core/       Shared types: channels, meters, console events (serialized to the UI)
  audio-engine/   Device discovery and multichannel metering (cpal)
  console/        ConsoleAdapter trait, dLive adapter + protocol, simulated console
  automix/        Auto-mix: room-feel presets, guardrails, the controller and its async loop
  store/          Local SQLite: settings and the auto-mix activity log
  auth/           AuthProvider trait, roles, and a LocalGuest placeholder
src-tauri/        The desktop app: Tauri commands and events that wire the crates to the UI
assets/brand/      SanctuaryMix wordmark (light and dark SVGs); assets/icon.svg is the app icon source
src/              React UI (views, components, Zustand store, backend bridge)
.github/workflows CI, security scans, dev builds and releases (see docs/CI.md)
docs/CI.md        How checks, git hooks, the dev channel and releases work
docs/AUTOMIX.md   How auto-mix decides, its guardrails, and known gaps
```

Platform-specific code is kept out of the crates, so a Windows build is the
same `npm run app:build` on a Windows machine.

### Adding another console

1. Add a module under `crates/console/src/` implementing `ConsoleAdapter`.
2. Add a variant to `ConsoleModel` and an arm in `create_adapter`.
3. Add it to the console picker in `src/views/SetupView.tsx` and `ConsoleModel` in `src/lib/types.ts`.

### Auth

`crates/auth` defines `AuthProvider` (`current_session`, `begin_sign_in`,
`complete_sign_in`, `sign_out`). A session carries the user, the active church
(organization) and the user's role there (Admin, Engineer, Volunteer), since
one engineer may serve several churches. The app currently uses `LocalGuest`,
which always signs in a local admin so the desk is never locked on a Sunday
morning.

Approved plan: local SQLite on each computer (works fully offline), Supabase
Postgres + Auth in the cloud for accounts and sync, sign-in in the system
browser (OAuth + PKCE) returning via a `sanctuarymix://` deep link, and tokens
stored in the macOS Keychain / Windows Credential Manager. A Supabase provider
replaces `LocalGuest` in `src-tauri/src/state.rs` without touching the UI.

## Building

### Prerequisites (Mac)

- Xcode Command Line Tools: `xcode-select --install`
- Rust: https://rustup.rs
- Node.js 22 (e.g. `brew install node@22`)

```sh
npm install
npm run app:dev        # run the app with hot reload
npm run app:build      # build SanctuaryMix.app and a .dmg for this Mac
npm run app:build:mac  # universal build (Apple Silicon + Intel); needs:
                       #   rustup target add aarch64-apple-darwin x86_64-apple-darwin
```

The app ends up in `target/release/bundle/macos/SanctuaryMix.app` (or
`target/universal-apple-darwin/release/...` for the universal build). Drag it
into Applications. Every CI run on GitHub also attaches a ready-made `.dmg`
under the run's **Artifacts**.

Builds are not code-signed yet, so the first launch needs a right-click on the
app, then **Open**. Signing and notarization will be added before handing it to
volunteers.

### Windows

Install Rust, Node 22 and the Visual Studio C++ build tools, then
`npm install && npm run app:build`. Tauri produces an `.msi` and an `.exe` installer.

### Tests

```sh
cargo test --workspace   # protocol encoding/decoding, metering math, adapters
npm run build            # typecheck + production UI build
```

## Connecting to a dLive

1. Connect the Mac to the same network as the MixRack's network port.
2. In Dante Controller, route the dLive's Dante outputs to Dante Virtual
   Soundcard, console input 1 to DVS input 1 and so on.
3. In SanctuaryMix, open **Setup**, choose *Dante Virtual Soundcard* and press
   **Start listening**.
4. Choose *Allen & Heath dLive*, enter the MixRack (or Surface) IP and the MIDI
   channel set under Utility › Control › MIDI on the console, and press **Connect**.

On connect, SanctuaryMix only asks the console for channel names. Nothing on the
desk changes until you move a fader or press a mute in the app.
Use *Simulated console* to try everything without hardware.
