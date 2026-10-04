---
name: sanctuarymix-design
description: Use when building, styling or reviewing any SanctuaryMix UI (screens, React components, colors, type, icons, UI copy, logo use) so it follows the SanctuaryMix design system.
---

# SanctuaryMix design

SanctuaryMix is a Tauri + React/TypeScript app for live church sound on an Allen & Heath dLive over Dante. Users are often volunteers, standing, in a dark booth, glancing up from the stage. Design for quick glances, dim rooms and plain words.

## Sources of truth (read before designing)

- `/mnt/project-files/design/GUIDELINES.md`: the full guidelines (principles, color, type, layout, voice, every component).
- `/mnt/project-files/design/tokens.json`: every token with its usage note. Edit this, then run `python3 build_tokens.py` in that folder to regenerate `tokens.css`.
- `/mnt/project-files/design/tokens.css`: the CSS variables the app imports. Copy it into the repo's styles folder when it changes.
- `/mnt/project-files/design/components.css`: reference `sm-` component styles.
- Design sheet with live previews: https://claude.ai/artifact/EtFY9gDukx62dGqZxjjVsm
- Logos: `/mnt/project-files/branding/logo/sanctuarymix-wordmark-{light,dark}.svg`. Use as supplied; never rebuild in live type or recolor.

If the shared folder isn't available (for example, working on the user's own machine), use the rules below and the copy of `tokens.css` in the repo.

## Hard rules

1. **Tokens only.** Every color, space, radius, size, shadow, duration and font comes from a CSS variable in `tokens.css`. No literal hex values, px font sizes or ad-hoc shadows in components.
2. **Dark is the default theme.** Set `data-theme` on `<html>` to `dark`, `light` or `system`. Check every new screen in both themes.
3. **Reserved hues mean one thing each:**
   - Red (`--mute`, `--meter-clip`, `--danger`): muted, clipping, or broken. No red fills for anything else, including delete buttons (those are outlined in `--danger`).
   - Yellow (`--solo`): solo/PFL only. Warnings use `--warning` amber with a word.
   - Blue (`--assist`, `--assist-subtle`): the AI assistant suggested or changed this. Nothing else is blue.
   - Green (`--success`, `--meter-signal`): connected, healthy signal.
   - Teal (`--accent`): primary action and selection. One primary button per view. Text on teal uses `--on-accent`; teal text uses `--accent-text`.
4. **Never color alone.** Every state also has a word, fill or position (Mute key says MUTE; status pills have a label).
5. **Contrast.** Text 4.5:1 on its surface in both themes; control borders, focus rings and meaningful icons 3:1. Use `--text`, `--text-muted`, `--text-subtle` (all pass on `--bg`, `--surface`, `--surface-raised`, `--control`). Decorative dividers use `--border`; control outlines use `--border-strong`.
6. **Focus** is a 2px `--focus-ring` outline with 2px offset on everything interactive.
7. **Sizes.** Default controls `--control-md` (40px). Anything tapped during a service is at least `--hit-min` (44px). Setup screens use `--control-lg` (48px).
8. **AI is visible and reversible.** Assist never changes a fader, mute or scene without Apply unless that channel's Auto mode is on. Anything Assist changed is outlined in `--assist` and has Undo.
9. **The console is the truth.** Mirror dLive channel names, numbers and colors (`--tag-*` as a 4px stripe on the name block). Show a state only after the console confirms it; pending changes keep the old state with a dashed `--border-strong` outline.
10. **No decorative motion.** Meters and faders follow the console in real time with no easing. Use `--duration-fast/base/slow` for UI only; they go to 0 under reduced motion.

## Surfaces

`--bg` (app background) → `--surface` (panels) → `--surface-raised` (strips, cards, dialogs) → `--control` / `--control-hover` (things you press). Panels use borders, not shadows; only popovers and dialogs use `--shadow-overlay`. Meters sit in a `--meter-track` well that is dark in both themes.

## Type

- Figtree for UI, IBM Plex Mono 500 for numeric readouts. Both bundled with `@fontsource`, never loaded from the web.
- Styles (as `.text-<name>` classes or `font: var(--text-<name>)`): `display` 32, `title` 22, `heading` 17, `body` 15 (the default), `body-strong` 15, `caption` 13, `label` 12 uppercase, `channel-name` 14, `readout` 13 mono, `readout-lg` 28 mono.
- Nothing below 12px. Sentence case everywhere except `label`.
- dB to one decimal with a real minus sign (U+2212): "−4.5 dB", "0.0 dB", "−∞". "Ch 12", "Mix 3", "DCA 2", "120 Hz", "2.5 kHz". Tabular figures for anything live.

## Spacing and shape

4px grid: `--space-1` 4 … `--space-8` 64. Inside controls `--space-2`/`--space-3`, panel padding `--space-4`, between sections `--space-5`. Radii: `--radius-sm` keys/badges/meters, `--radius-md` buttons/inputs/strips, `--radius-lg` panels/dialogs, `--radius-pill` only status pills and the Assist badge. Channel strips are `--strip-width` (88px).

## Layout

Window minimum 1280 × 800. Top bar 56px (logo, status pills, Assist mode), left nav rail 72px (Mixer, Scenes, Assist, Setup), mixer bay on `--bg` scrolling horizontally inside its own region, inspector 320px on the right.

## Components

Build these from `components.css` and the guidelines; don't invent parallel versions: Button (primary / default / ghost / danger outline; sm/md/lg), ChannelKey (Mute/Solo/Select latching keys, `aria-pressed`), Meter (dBFS zones fixed by height: signal below −12, hot −12 to −3, clip above −3; clip latch), ChannelStrip (name block with tag stripe, meter + fader with unity at 75%, value, keys; muted shows a red top bar), StatusPill (dot + word), AssistCard (Assist badge, plain title, exact before → after in mono, one-sentence reason, Apply/Dismiss, then Undo), TextField (label above, helper/error below, validate on blur), SegmentedControl (2 to 4 options).

## Voice

A calm, experienced sound tech helping a first-timer. Buttons are verbs ("Connect", "Recall scene", "Apply"), never "OK". Errors say what happened and what to do. Assist explains itself in one sentence a volunteer can check by ear. No emoji, no exclamation marks.

## Icons

`lucide-react`, 20px, `strokeWidth={1.75}`, `currentColor`, before the label with `--space-2` gap. Icon-only buttons need `aria-label` and a tooltip.

## When changing the system

Change `tokens.json` first, run `build_tokens.py`, check contrast in both themes, update GUIDELINES.md and the design sheet, and copy the new `tokens.css` into the repo. A new meaning never reuses a reserved hue.

## Review checklist

- [ ] No literal colors, font sizes or shadows; all from tokens
- [ ] Looks right in dark and light
- [ ] Reserved hues used only for their meaning; every state has a word
- [ ] Live controls ≥ 44px; visible focus ring
- [ ] Numbers in mono with real minus signs and units
- [ ] Copy is plain, verb-first, no jargon beyond the console's own
- [ ] Anything AI-driven is blue-outlined and undoable
