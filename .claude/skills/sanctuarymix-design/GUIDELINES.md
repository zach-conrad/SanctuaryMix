# SanctuaryMix design guidelines

Design sheet (live previews, both themes): https://claude.ai/artifact/EtFY9gDukx62dGqZxjjVsm  
Files in this folder: `tokens.css` (import this in the app), `tokens.json` (source of truth), `components.css` (reference component styles), `build_tokens.py` (regenerates tokens.css), `SKILL.md` (the design skill for Claude threads).

SanctuaryMix is a live mixing app for church sound teams. It runs on a Mac in the booth, talks to an Allen & Heath dLive over Dante, and offers AI mixing suggestions. The people using it are often volunteers, standing, in a dark room, glancing up from the stage. Every rule below serves that person.

## Principles

1. **Readable at a glance.** Any state that matters during a service (muted, soloed, clipping, offline, Assist waiting) is readable in under a second from arm's length. State is shown by fill, position and a word, never by color alone.
2. **Dark by default.** The `dark` theme is the default because the booth is dark during worship. `light` is for daytime setup and rehearsal. Both are first-class and every pair below passes contrast in both.
3. **Volunteer first.** Plain words a newcomer understands ("Mute", "Pastor Lav", "Connect to your console"). Jargon only where the console uses it, and then exactly as the console spells it.
4. **The console is the truth.** Channel names, numbers and colors mirror the dLive. A control shows a state only after the console confirms it.
5. **AI is visible and reversible.** Anything Assist suggests or changes wears the `assist` hue and can be undone in one tap.
6. **Feels like a Mac.** Follow macOS patterns people already know (System Settings, Finder, Mail): a source-list sidebar, grouped lists, controls that step back so the content leads, and the same pattern everywhere. Reasoning in `apple-direction.md`.
7. **Nothing moves unless the sound does.** No decorative animation. Meters and faders track the console in real time with no easing.

## Color

The palette starts from the logo: `brand-ink` (#15171C) and `brand-paper` (#F3F2EE) become the surfaces, and the logo's teal becomes `accent` (`brand-teal` #0E7C7B in light, `brand-teal-bright` #3FC1BE in dark).

**Surfaces, back to front:** `bg`, `surface` (top bar, sidebar, inspector), `surface-raised` (panels, groups, strips, dialogs), then `control` for things you press and for row hover. Panels and groups are told apart by fill, not outlines; use `border` for hairlines between rows and regions, and `border-strong` to outline controls (it holds 3:1). A border on a panel always means something: focus, selection, AI, or danger. Only popovers and dialogs take `shadow-overlay`.

**Text:** `text` on every surface; `text-muted` for secondary lines; `text-subtle` for scale numbers, timestamps and placeholders. All three hold 4.5:1 on `bg`, `surface`, `surface-raised` and `control` in both themes. Teal text uses `accent-text`, never `accent`.

**Accent:** teal means "this is the main action" or "this is selected": the primary button, the selected channel, the active tab, a toggle that is on. One primary button per view. Text on teal fills is `on-accent` (dark ink in the dark theme, white in light).

**Reserved meanings.** These hues mean one thing each, everywhere, so a volunteer learns them once:

| Hue | Token | Means | Never use for |
| --- | --- | --- | --- |
| Red | `mute`, `meter-clip`, `danger` | Muted, clipping, or broken | Decoration, delete buttons as fills |
| Yellow | `solo` | Solo / PFL is on | Warnings (those use `warning` amber, with a word) |
| Blue | `assist` | The AI assistant suggested or did this | Links, info, selection |
| Green | `success`, `meter-signal` | Connected, healthy signal | Primary actions |
| Teal | `accent` | Primary action, selection | Status |

**Meters** sit in a `meter-track` well that is dark in both themes, with fixed zones: `meter-signal` below -12 dBFS, `meter-hot` from -12 to -3, `meter-clip` above -3. See Meter.

**Channel colors.** The dLive lets engineers color channels Red, Green, Yellow, Blue, Light Blue, Purple, White or Off. Show the console's choice as a 4px stripe on the strip's name block using `tag-red` through `tag-white`. Never fill a whole strip with a tag color.

## Type

**Figtree** (the logo's typeface) for all interface text; **IBM Plex Mono** at 500 for numeric readouts so values don't jitter as they change. Both are bundled with the app (via `@fontsource`), never loaded from the web: the booth may have no internet.

- Screen titles in `title`; panel and dialog headings in `heading`.
- Running text in `body` (15px). Nothing smaller than `caption` (13px) for sentences, and nothing smaller than `label` (12px) at all.
- Uppercase only in `label`, for console-style section labels ("INPUTS", "MIX 3").
- Channel names in `channel-name`. Values in `readout`; the selected channel's big value in `readout-lg`.
- Sentence case for everything else, including buttons and headings.

## Numbers and units

- dB to one decimal with a real minus sign: "−4.5 dB", "0.0 dB", "+2.0 dB". Fully down is "−∞".
- Frequencies as "120 Hz" and "2.5 kHz"; times as "12 ms".
- Channels as "Ch 12", mixes as "Mix 3", DCAs as "DCA 2", scenes as "Scene 14 · Sunday 9am".
- Always tabular figures (`font-variant-numeric: tabular-nums`) for anything that updates live.

## Spacing, size and shape

- A 4px grid: `space-1` (4) to `space-8` (64). Inside controls use `space-2` and `space-3`; inside panels `space-4`; between sections `space-5`.
- Buttons and inputs are `control-md` (40px). Anything a volunteer taps during a service is at least `hit-min` (44px); setup screens use `control-lg` (48px).
- Corners: `radius-sm` for keys, badges and meters; `radius-md` for buttons, inputs and strips; `radius-lg` for panels and dialogs; `radius-pill` only for status pills and the Assist badge.
- Channel strips are `strip-width` (88px). The mixer bay scrolls horizontally inside its own region; the window never does.

## Layout

The main window, minimum 1280 × 800:

- **Top bar** (56px, `surface`): logo at left, then status pills (console, Dante, warnings) with the Assist or Auto-mix badge at the end of that row; at the right, Freeze (only while auto-mix runs) right beside Record service.
- **Sidebar** (source list, `sidebar-width` 200px, `surface`): icon + name for Mixer, Scenes, Services, Assist, Setup, with Settings pinned at the bottom. The current page gets an `accent-subtle` fill. Below 1440px wide it folds to a 72px icon rail so the mixer keeps its room.
- **Mixer bay** (`bg`): channel strips on `surface-raised`, grouped by the console's layers.
- **Inspector** (right, 320px, `surface`): the selected channel's detail and the Assist panel.

**Pages** (everything except the mixer):

- Every page opens with a page header: the title in `title` and any page-wide action at the right. No description line unless the page can't be understood without it.
- Content centers in the window and never grows wider than `page-max` (1200px). Pages made of forms and settings (Setup, Settings, Scenes) use one column at `page-narrow` (760px).
- When a page has a main list or table plus supporting panels (status, history, settings), put the supporting panels in a side column `side-width` (360px) on the right. Don't lay panels out in a free-flowing grid.
- Settings and records are **grouped lists**, as in macOS System Settings: a small bold section heading outside the box, then a rounded `surface-raised` group of rows. Each row has its label (regular weight, optional caption) on the left and its control at the right edge, with a hairline (`border`, inset from the left) between rows. The group's action goes in a last row: state on the left, button on the right. One footnote line under the group at most.
- Lists of records (services, scenes) are one group per date or category with hairline rows, not a bordered card per item.
- Picking one item from a short list (audio devices) is a row with a teal checkmark on the chosen one, not radio cards.
- Empty pages show a centered symbol, a one-line title and one line of text.
- Reference text people read once (rules, explanations) sits in a collapsed disclosure, not an always-open panel.

**Choosing a control for options:** two to four short options are a SegmentedControl. Five or more, or options that each need a sentence of explanation, are a dropdown (`select`) in a row, with the chosen option's explanation as the group's footnote. Picking one item from a short list of things (devices) uses checkmark rows. Don't lay out a grid of cards for a single choice.

## Interaction and states

- **Focus:** a 2px `focus-ring` outline, 2px offset, on every interactive element. Never remove it.
- **Hover** lightens fills one step (`control` to `control-hover`, `accent` to `accent-hover`) over `duration-fast`.
- **Pending:** while the console hasn't confirmed a change, keep the old state and add a dashed `border-strong` outline. After 2 seconds without confirmation show a `warning` pill.
- **Undo** over confirm: live actions (mute, fader, scene recall) happen immediately and offer Undo. Only destructive setup actions (forget a console, overwrite a scene) ask first.
- **Motion:** `duration-fast` for hover, `duration-base` for panels, `duration-slow` at most. All durations drop to 0 under `prefers-reduced-motion`.

## Voice

Write like a calm, experienced sound tech helping a first-timer.

- **Fewer words.** Labels are one to three words. A panel gets at most one short line of help, and only when the control can't be used without it. Explanations of how something works go in a tooltip (`title`), not on screen. States are a word or two ("Off", "Frozen", "Riding 10 channels"), not sentences.

- Name things the way the room does: "Pastor Lav", "Worship Lead", "the band", not "Input 1 (dyn mic)".
- Buttons are verbs: "Connect", "Recall scene", "Apply". Never "OK" or "Submit".
- Errors say what happened and what to do: "Can't reach the dLive at 192.168.1.70. Check the network cable or pick it again from the list."
- Assist explains itself in one sentence a volunteer can check by ear: "Speech has sat 3 dB under your target for 20 seconds."
- No emoji, no exclamation marks, no blame.

## Iconography

Use **Lucide** (`lucide-react`, ISC license): 20px, 1.75px stroke, `currentColor`, round caps. Icons sit before their label with `space-2` between. An icon alone needs an `aria-label` and a tooltip, and is never the only way to tell Mute, Solo or a status apart.

## Logo

Use the files under Logos exactly as supplied: `sanctuarymix-wordmark-dark.svg` on dark grounds, `sanctuarymix-wordmark-light.svg` on light grounds. Minimum width 120px, clear space equal to the height of the "s" on every side. Don't recolor, outline, or set the name in live type in place of the logo.

## Using the tokens in the app (summary)

The React UI imports `tokens.css` once at the root. It defines every token as a CSS variable (`var(--surface)`, `var(--space-4)`, `var(--text-body)`), with `dark` on `:root` and `light` under `[data-theme="light"]`; `data-theme="system"` follows the OS. Component classes use the `sm-` prefix (see the Components section and `bundle.css`). Never write a literal hex value in a component.

## Setting up the React UI

```ts
// src/main.tsx
import "@fontsource/figtree/400.css";
import "@fontsource/figtree/600.css";
import "@fontsource/figtree/700.css";
import "@fontsource/ibm-plex-mono/500.css";
import "./styles/tokens.css";      // copy of design/tokens.css
import "./styles/components.css";  // optional: the sm- reference styles
```

- Theme: set `document.documentElement.dataset.theme` to `"dark"`, `"light"` or `"system"`. Store the choice in app settings; default `dark`.
- In components, use `var(--token)` in CSS modules or `style={{ color: "var(--text-muted)" }}`. Never a literal hex, px font size, or shadow.
- Type styles: either the `.text-body` / `.text-readout` classes, or `font: var(--text-body); letter-spacing: var(--text-body-tracking);`.
- Icons: `lucide-react`, `size={20}`, `strokeWidth={1.75}`.
- Meter ballistics run on a `requestAnimationFrame` loop writing `--level` and `--peak` to the element's style, not React state per frame.

## Components

### ChannelStrip

One input or mix channel in the mixer bay: name block, meter, fader, value and keys, 88px wide (`strip-width`).

**Parts, top to bottom.** Name block on `control` with a 4px stripe in the channel's dLive color (`tag-*` tokens; no stripe when the console color is Off) and the channel number in `readout` mono. Meter and fader side by side. The fader value in `readout` with a real minus sign and "−∞" when fully down. Mute and Solo keys.

**States.** Selected: `accent` outline and `accent-subtle` name block. Muted: a 3px `mute` bar on the strip's top edge, so a muted channel is visible even when the keys are scrolled out of view. Changed by Assist: the fader cap is outlined in `assist` until the user touches it.

**The consumer provides** the channel number, the console's name (wraps to two lines, then truncates; full name in a tooltip), its color, level, fader position (0.75 = unity, marked by the line at 75%) and states.

**Don't** recolor the strip by channel type, or put any control above the name: the name is what volunteers scan for.

### ChannelKey

Latching keys on a channel strip that mirror the console: Mute, Solo (PFL) and Select.

Each key is a `<button aria-pressed>`; the label never changes, the fill does. Engaged Mute fills with `mute` and `on-mute`; engaged Solo fills with `solo` and `on-solo`; engaged Select fills with `accent`. Unlit keys use `control` with a `border-strong` outline and `text-muted` label, in the `label` style (12px, uppercase).

**The consumer provides** the console's confirmed state. Show a key as engaged only after the dLive confirms it; while the change is in flight, keep the old fill and add a 2px `border-strong` dashed outline.

**Size.** At least `hit-min` (44px) tall outside the mixer bay; 32px inside a strip, where the whole strip column is the target width.

**Don't** reuse red or yellow fills anywhere else, or replace the words with icons only.

### Meter

A vertical dBFS level meter, drawn in a dark well in both themes so it reads like the console.

**Scale.** Height maps to dBFS: 0 at 100%, −3 at 92%, −6 at 85%, −12 at 72%, −18 at 60%, −30 at 40%, −48 at 18%, −60 at 0. Color is fixed by height, not by level: `meter-signal` below −12, `meter-hot` from −12 to −3, `meter-clip` above −3. The top 6px is the clip latch; it lights `meter-clip` at 0 dBFS and holds until the user clicks the meter.

**The consumer provides** `--level` and `--peak` (0 to 1, already mapped through the scale) and `data-clipped`. Peak hold is a 2px `text` line that holds 1.5 s then falls.

**Ballistics.** Update at display rate with the console's meter data. No CSS transitions on the fill: a meter that eases lies.

Width is `meter-width` (8px); stereo is two meters with a 2px gap.

### AssistCard

A suggestion from the AI mixing assistant, waiting for the operator's decision.

Uses the reserved `assist` hue: `assist-subtle` background, `assist` outline, Assist badge. The title names the action in plain words ("Raise Pastor Lav"); the change line states the exact before and after in `readout` mono; the reason is one sentence a volunteer can check by ear.

**The consumer provides** the title, the exact change, the reason, a timestamp, and Apply / Dismiss handlers. Apply is the primary button; after applying, show "Applied · Undo" in place of the actions for 10 seconds.

**Rules.** Assist never moves a fader, mute or scene without Apply unless the channel's Auto mode is on. Every automatic change is outlined in `assist` on the affected control and listed in the Assist panel with Undo. Suggestions expire after 30 seconds if the condition has passed.

### StatusPill

A compact readout of system state in the top bar: console connection, Dante clock, current scene.

Always a word plus a dot; the dot color alone never carries the meaning. Tones: `sm-pill--ok` (`success` dot), `sm-pill--warn` (`warning` dot), neutral (no modifier, `text-subtle` dot), `sm-pill--error` (`danger` text and outline, for states that stop the show).

**The consumer provides** the subject ("dLive DM64", "Dante") and a short state in `sm-pill__meta` ("Connected", "Clock not locked"). Clicking a pill opens the matching setup panel.

**Don't** use pills as buttons for actions, or stack more than four in the top bar.

### Button

Buttons start an action: connect, recall a scene, apply a suggestion.

**Variants.** `sm-btn--primary` (teal `accent` fill, `on-accent` label) for the one main action on a screen or dialog; plain `sm-btn` (`control` fill, `border-strong` outline) for everything else; `sm-btn--ghost` for Cancel and low-stakes actions; `sm-btn--danger` (outlined in `danger`) for Forget, Delete and Disconnect. There is no filled red button: red fills mean Mute.

**Sizes.** Default 40px (`control-md`). Use `sm-btn--lg` (48px, `control-lg`) for setup screens and anything pressed during a service. `sm-btn--sm` (32px) only in the mouse-driven inspector.

**The consumer provides** a verb-first label in sentence case ("Recall scene", not "OK"), an optional 20px Lucide icon before the label, and `disabled` when the action can't run, with the reason shown nearby.

**Don't** put two primary buttons side by side, or use a button for a state (use a channel key or segmented control).

### TextField

A labeled single-line input for setup screens: console name, IP address, Dante device names.

Label above in 13px semibold `text`; input 40px on `surface-raised` with a `border-strong` outline; helper text below in `caption` `text-muted`. Invalid: `danger` outline and the helper becomes the error, saying what is wrong and how to fix it ("Each number must be 0 to 255").

**The consumer provides** the label, value, optional placeholder (an example value in `text-subtle`, never the instructions) and the helper or error text.

**Don't** use placeholders as labels or validate while the user is still typing; validate on blur.

### SegmentedControl

Two to four mutually exclusive options shown together: mixer layer, Assist mode, theme.

Track on `control`; the chosen segment lifts to `surface-raised` with a `border-strong` inset and `text` label; others are `text-muted`. Each segment is a `<button aria-pressed>` inside a `role="group"` with an `aria-label`.

**The consumer provides** the options (one or two words each) and the selected value.

**Don't** use it for more than four options (use a menu) or for actions.

## Changing the system

Edit `tokens.json` first, run `python3 build_tokens.py` to regenerate `tokens.css`, then republish the design sheet so both stay in step. New colors need a contrast check (4.5:1 for text, 3:1 for control edges and meaningful marks) in both themes. Don't add a new hue for a new meaning without updating the reserved-meanings table.
