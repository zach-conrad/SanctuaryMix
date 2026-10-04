# Auto-mix

Auto-mix rides the input faders the operator picks so the mix sounds the same
every week, whoever is in the booth. It covers levels and balance only; EQ
comes later. The numbers come from the project's church live-mix research
(`research/live-mix/church-auto-mix-research.md` in the project files).

## How it decides

- Each picked channel gets a role (speech, lead vocal, backing vocal, kick,
  and so on). Roles are guessed from the console's channel names and can be
  changed on the Assist screen.
- The operator picks a room feel: Full and modern (default), Big and loud
  (admins only, with a hearing warning), Deep low end, Warm and intimate,
  Traditional and choral, or Spoken word. Three nudges of up to 3 dB each
  adjust Loudness, Low end and Vocal presence.
- Speech mics and lead vocals are anchors: they ride toward the feel's
  reference level, so the pastor and worship leader land in the same place
  every week.
- Everything else rides toward its place relative to the loudest lead vocal
  that is singing, so builds and quiet verses keep their shape. With no lead
  vocal singing, the band is left where it is.
- While a speech mic is talking, music steps back to sit 15 dB under it
  (18 dB in Spoken word).
- Levels are estimated post-fader: the Dante input's RMS (3 second window)
  plus the fader position. Without a calibrated room mic the app manages
  balance only and leaves the main fader to the operator.

## Guardrails

All enforced in Rust (`crates/automix/src/guardrails.rs`), not the UI. Settings
from the UI are clamped to hard limits, and the choke point clamps again.

| Rule | Music | Speech |
| --- | --- | --- |
| Largest step | 0.5 dB | 1 dB |
| Fastest travel | 1.5 dB/s | 3 dB/s |
| Dead band | ±1 dB | ±1.5 dB |
| Range around the operator's position | −12 to +6 dB | −10 to +8 dB |

- Never above 0 dB unless the operator already had it there (then +10 dB at most).
- Never pulls a fader to −∞.
- Moves in whole console steps (0.5 dB on dLive).
- Only fader moves exist in the code path to the console (`FaderSink`): no
  mutes, unmutes, gain, routing or scenes. Unpicked channels are never touched.
- Muted channels are left alone, and so is a channel for 5 seconds after an unmute.
- Never raises a mic nobody is using (the main feedback guard), never raises a
  channel peaking above −3 dBFS, and leaves a clipping input alone.
- Holds everything when Dante audio stops or the console disconnects. On
  reconnect it adopts whatever the desk reports; it never snaps back.
- A person always wins. Moving a picked fader hands that channel back to them
  (their position becomes the new baseline) until they press Hand back.
  Freeze (top bar, or Esc) stops every automatic move at once. Undo puts a
  channel back where the operator set it and holds it there.
- Every move, takeover, freeze and undo is logged in the local SQLite database
  with a one-sentence reason.

## Code map

- `crates/automix`: presets (`preset.rs`), guardrails, the controller
  (`engine.rs`, synchronous, time passed in) and the async loop (`runner.rs`).
- `crates/store`: local SQLite (settings and the auto-mix log).
- `src-tauri/src/state.rs`: wires the loop to the console and the UI;
  `commands.rs` has the `automix_*` commands.
- `src/components/AutoMix.tsx`, `src/store/automix.ts`: the Assist screen.
  `src/lib/demoAutomix.ts` is a simplified stand-in for the browser demo only.

## Known gaps

- **Not tried on a real dLive yet.** Tests run against the simulated console.
- **dLive fader positions.** The adapter doesn't ask the desk where faders sit
  (the simulated console does). On a dLive, a picked channel shows "Needs a
  nudge" until its fader moves once on the desk or in the app.
- **Dante patch.** Audio input N is assumed to be console input N.
- **dLive automatic mic mixer.** The research recommends the dLive's built-in
  AMM for speech gain sharing. Whether it can be configured over the MIDI/TCP
  protocol still needs checking against Allen & Heath's protocol document and
  a real desk; this PR doesn't use it.
- Later: feedback detection, measurement-mic calibration, per-channel feedback
  ceilings from a ring-out, week-to-week baseline learning, service segments.
