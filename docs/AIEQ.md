# AI EQ

AI EQ listens to each input auto-mix looks after and helps its tone: it
proposes gentle EQ at soundcheck, catches feedback during the service, and
keeps speech mics sounding like themselves. It works on the desk's own input
EQ (dLive PEQ and high-pass filter), so everything it does is visible and
editable on the console. The rules come from the project's AI EQ docs
(`aieq/behavior.md`, `aieq/architecture.md` and `aieq/design/` in the project
files), taking every recommended option.

Code: the `tonal` crate (analysis, solver, guardrails, controller), the EQ
parts of `mix-core` and `console`, `src-tauri/src/aieq.rs`, and the AI EQ
screens in `src/`.

## Who decides what

| | AI EQ does it | A person decides |
|---|---|---|
| Soundcheck proposal | Listens, proposes | Apply, Apply all, Skip, Keep my EQ |
| Feedback | Notch on band 4 and pulls the fader, straight away | Undo |
| Speech tone keeping | Small moves on speech mics during the service | Switch it off (Engineers, Admins) |
| A person's EQ change | Stops touching that channel (except feedback) | Hand back |
| Feedback check (ring-out) | Raises one mic at a time until it rings | Start it, every time (Engineers, Admins) |
| Ideas for next week | Collects them when the service ends | Keep or dismiss (Engineers, Admins) |

AI EQ has its own on/off, separate from auto-mix, and uses auto-mix's
channel picks and roles (one pick for both). It needs the Pro plan; Undo,
Freeze, hand back and EQ by hand are never locked. **Freeze stops every AI
move**, faders and EQ.

## How it hears

- The Dante feed is tapped before the desk's EQ (the normal dLive patch). The
  app keeps a copy of each channel's desk EQ and models it in software (RBJ
  biquads at 48 kHz, `crates/tonal/src/model.rs`; the UI draws the same
  curves from `src/lib/eqCurve.ts`). Post-EQ taps work too (Settings).
- The analyser (`analyser.rs`) runs on its own thread: 4096-point FFT, hop
  1024, a 30 second long-term spectrum in 60 sixth-octave bands from 20 Hz.
  Voice mics only count while the listening models hear their own voice on
  them, so band bleed doesn't skew a pastor's mic.
- The feedback detector (`feedback.rs`) looks for one narrow line standing
  15 dB over the average, 12 dB over its neighbours and 25 dB over its
  harmonics, holding or growing for 0.3 s. Harmonics are what keep a held
  note or an organ pedal from counting.

## Soundcheck

1. Soundcheck listens to every picked channel (speech first, then vocals,
   then the band). After 15 seconds of clean sound a channel is Done (with a
   proposal) or Sounds good; a channel with nothing after a minute shows No
   sound.
2. The solver (`solver.rs`) compares what the mic hears with a target curve
   for its role (`targets.rs`, starting points to tune in beta) and places up
   to three wide bells on bands 1-3, cuts first. Boosts only add clarity to
   voices (2.5-6 kHz). It may suggest a high-pass under the role's ceiling.
   The Vocal presence and Low end nudges shape the targets (±2 dB), at
   soundcheck only.
3. Nothing reaches the desk until a person taps Apply. The log records AI as
   the author and the person who tapped Apply.
4. Before | After flips the desk between the two EQs. Not during a service.
5. Last Sunday's EQ for each mic (by channel name) is saved when a service
   ends. If the desk differs on the next soundcheck, the screen offers to put
   it back.

## During the service

- **Feedback**: a ringing channel gets a narrowest-width notch on band 4 at
  −3 dB, then −6, then −9 while it keeps ringing, and its fader comes down
  3 dB (up to 6 dB per ring) through auto-mix, whether or not auto-mix is on.
  Auto-mix then holds every raise for 10 s and never raises that fader above
  where it rang. When band 4 is a person's, only the fader moves. Notches
  keep working on channels a person has taken over.
- **Speech tone keeping** (on by default): speech mics only, gain only,
  0.5 dB at a time, at most once per 10 s per band, within ±3 dB of the
  soundcheck EQ, at most 6 messages a minute per channel.
- **Hands off**: any EQ change a person makes, on the desk or in the app,
  makes that channel theirs (shown as Yours) until someone hands it back.
- **Undo** puts a channel back to its soundcheck EQ and holds it there.
- Soundcheck changes, Before | After and the feedback check wait for the
  service to end.

A service is under way while a recording runs or auto-mix is on (the same
rule as sign-out and plan changes).

## Hard limits

Enforced in Rust (`crates/tonal/src/guardrails.rs`), never in the UI:

- No band more than +3 dB over where it was before AI EQ, nor over +6 dB.
- Tone cuts no deeper than −6 dB; notches no deeper than −9 dB.
- Tone bands at least 2/3 octave wide; band 4 is kept for feedback notches.
- No boost within 1/6 octave of a frequency that has rung on that mic.
- High-pass ceilings per role (speech 150 Hz, vocals 120 Hz, acoustic
  100 Hz, electric 90 Hz, drums 100 Hz, keys 60 Hz, kick and bass 30 Hz;
  never on organ, tracks or ambience); a person's high-pass is never turned
  off or moved.
- At most 20 EQ messages a second to the desk; no tone move within 1 s of a
  fader move on the same channel; auto-mix waits 3 s after an EQ change
  before reacting to that channel.

## Feedback check (ring-out)

Before the service, an Engineer or Admin can run the feedback check (the
screen asks every time). It raises one vocal or speech mic at a time by
0.5 dB a second, up to 10 dB over where it was (never above +6 dB), until it
rings. It puts the fader straight back, records where it rang, and gives
auto-mix a ceiling 3 dB under that level. Frequencies that rang are kept so
soundcheck never boosts near them.

## The EQ log

Every EQ change is logged with what changed, why, who made it and, for
soundcheck, who applied it. The fields match `eq_audit` and `eq_ideas` in
`docs/supabase/eq_audit.sql`, so the website's EQ history can take them as
they are once recordings sync:

| Field | Values |
|---|---|
| `action` | `soundcheck`, `feedback`, `tone`, `undo`, `person`, `handBack` |
| `changes` | lines like `320 Hz  0.0 → −2.0 dB`, `Low cut  off → 100 Hz` |
| `by_kind` / `by_where` | `ai` or `person`; `app` or `desk` |
| `by_role`, `by_user` | the person's role and account (the desk never says who) |
| `applied_role`, `applied_user` | who tapped Apply on a soundcheck change |

Locally they live in the `eq_log` and `eq_ideas` tables of
`sanctuarymix.db` (`crates/store`). A recorded service's EQ history
(`aieq_audit`) is every entry from the morning of its service date to its
end, timed from the recording's start (soundcheck before it has no time).
Cloud sync is not built yet.

## Ideas for next week

When a service ends AI EQ collects ideas: keep a notch that rang, remove a
saved notch that wasn't needed, or carry 1 dB of a speech mic's tone drift
into next week. Keeping one changes that mic's saved EQ.

## Not built yet

- Sharing anonymous EQ data to improve the targets (off and opt-in when it
  comes).
- Voice ID: a mic is known by its channel name.
- Asking Claude about a channel's tone.
- Consoles other than dLive (and the simulated desk). On a desk without EQ
  control, AI EQ shows Not supported and changes nothing.
- The target curves and filter shapes are starting points: they should be
  checked against a real dLive (pink noise through each width and type) and
  tuned in beta.
