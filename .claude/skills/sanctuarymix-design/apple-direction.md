# SanctuaryMix: Apple-style direction (proposal)

Status: approved by Zach 2026-10-04 and written into SKILL.md and GUIDELINES.md. Built on PR #18.
Mockups: /mnt/project-files/screenshots/apple-redesign/ (dark, light, 1280 wide, plus before/after compares).

## What Apple does that we borrowed

Apple's macOS guidance comes down to three ideas: **hierarchy** (controls step back so content leads), **harmony** (shapes and spacing that feel like one system) and **consistency** (the same pattern everywhere, so nothing has to be learned twice).

| Apple idea | What it looks like in SanctuaryMix |
|---|---|
| Source-list sidebar (Finder, Mail, System Settings) | The left rail is now a 200px sidebar with icon + name. Below 1440px wide it folds back to the icon rail so the mixer keeps its room. |
| Grouped inset lists (System Settings) | Setup, Settings, Services, Recording, Disk space, Room feel and Coming next are rounded groups of rows. Label on the left, control on the right, hairline between rows. |
| Section header outside the box | Small bold heading above each group ("Console", "Dante audio", "Sunday, October 4") instead of a title inside a bordered card. |
| Fewer borders, more fill | Panels lost their outlines; a slightly lighter fill does the separating. Borders now mean something (focus, selection, AI blue, danger). |
| Checkmark for one choice | The Dante device list uses a teal checkmark on the chosen row, not radio buttons in boxed cards. |
| Footnotes instead of help text | One muted line under a group ("1.0 GB per hour.", the room-feel description). Longer explanations live in tooltips. |
| One list per day, not a card per item | Services are one group per date with rows split by hairlines. |
| Calm empty states | Scenes shows a centered symbol, a title and one line. |
| Toolbar holds the actions | Status pills on the left, Freeze + Record service on the right, nothing else. |

## What stayed the same on purpose

Dark default, the color rules (red = mute, yellow = solo, blue = AI only), 44px live controls, the mixer strips and the Inspector. Live mixing still wins over looks.

## Proposed rule changes for the design skill

1. **Layout:** source-list sidebar at 200px (`--sidebar-width`), collapsing to the 72px rail under 1440px.
2. **Forms and settings:** always a grouped list: `.section` > `.section-head` > `.group` > `.row`. Labels are regular weight; the control sits at the right edge.
3. **Panels:** filled `--surface-raised`, no border. Use a border only to mean focus, selection, AI or danger.
4. **Single choice from a short list:** a row with a checkmark. Five or more options: a dropdown (unchanged).
5. **Help text:** at most one footnote line under a group; everything else goes in a tooltip.
6. **Lists of records** (services, later scenes): one group per date or category with hairline rows, not separate cards.
7. **Empty states:** symbol, title, one line.
8. **Top bar:** pills left (console, Dante, warnings, Assist badge last); Freeze then Record service at the right. This fixes the stale top-bar line in GUIDELINES.md.

## Ideas not built yet

- Sidebar counts (for example "Services 7") and a red dot on Services while recording.
- Light vibrancy (frosted sidebar and toolbar) once we're inside the real Mac window; it doesn't render in the browser demo.
- Keyboard: Cmd-1..5 for the sidebar, Cmd-, for Settings, as Mac apps do.
- A compact Inspector variant that uses the same grouped rows.

## Sources

- Hierarchy, harmony, consistency: https://www.createwithswift.com/liquid-glass-redefining-design-through-hierarchy-harmony-and-consistency/
- macOS layout, sidebar widths and spacing: https://skills.cat/skills/ehmo/platform-design-skills/macos-design-guidelines
