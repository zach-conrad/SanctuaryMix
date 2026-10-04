# Room level (SPL)

SanctuaryMix shows how loud the room is, A and C weighted, from one input on
the Dante stream. The readout sits at the top of the mixer's inspector; click
it for a graph, averages and the settings.

## What to measure

Pick a **measurement mic out in the room** (an omni measurement mic at the mix
position, patched to a dLive input and sent over Dante) under *Measurement
input*. A stage mic or the main mix bus will not tell you how loud the room is.

## Calibration

Readings are dBFS plus an offset. Until the offset is calibrated, they are
labelled *uncalibrated* and are only good for comparing louder and quieter
moments. To calibrate:

1. Hold a sound level meter (set to slow) next to the measurement mic, or put a
   94 dB calibrator on it.
2. Play steady pink noise through the system (skip this with a calibrator).
3. Enter the meter's reading, pick dBA or dBC to match it, and press Calibrate.

The guided calibration (`crates/audio-engine/src/spl/calibrate.rs`) then:

- lets the weighting filter settle for half a second;
- measures the weighted level in quarter-second steps and waits for **5 seconds
  that stay within 1.5 dB**, restarting the count if the level jumps;
- refuses input that is **clipping** (raw peaks at or over −0.2 dBFS) or
  **too quiet** (under −65 dBFS, where noise and hum start to count);
- averages the steady stretch's energy and sets the offset so it reads the
  reference;
- gives up after 30 seconds with the reason.

The result is saved in the app database (`spl.config`) with the input, audio
device, sample rate, weighting, reference and time. If the input, device or
sample rate in use stops matching, readings go back to *uncalibrated* until you
calibrate again. Typing an offset by hand also clears the calibration.

Not detected yet: a change to the measurement mic's **preamp gain** on the
console (the dLive adapter doesn't report gain), so recalibrate after changing
it. Correcting for the measurement mic's own frequency response (its
manufacturer calibration file) is a possible follow-up.

## What it reports

| Value | Meaning |
|---|---|
| Fast / Slow | 125 ms and 1 s exponential time weighting (what a handheld meter shows) |
| Leq 1 min, Leq 15 min | Energy average over the window; 15 min is the usual window for church loudness guidelines |
| Leq since reset | Energy average since *Reset averages* (do it before the service) |
| Loudest (LAFmax) | Highest fast A level since the reset |
| C peak (LCpeak) | Highest instantaneous C-weighted peak since the reset |

The graph plots one-second Leq values for A and C. Two hours are kept.

## How it works

- `crates/audio-engine/src/spl.rs`: A and C weighting filters (IEC 61672
  analog poles, bilinear transform with the 12.2 kHz pole pre-warped; within
  Class 1 tolerance at 44.1 kHz and up), time weighting, Leq windows and
  history. Plain DSP, tested against the standard's table.
- `crates/mix-core/src/spl.rs`: the shared types (`SplConfig`, `SplReading`,
  `SplPoint`). Anything that wants room loudness, such as auto-mix loudness
  targets, uses these.
- `src-tauri/src/spl.rs`: runs the meter on the metering thread (audio still
  only enters through `start_metering`, so it inherits the microphone
  permission gate), emits the `spl` event about 10 times a second, and holds
  the `spl_*` commands. `AppState::spl.latest()` gives Rust code the newest
  reading.
- UI: `src/components/SplMeter.tsx`, `src/store/spl.ts`, `src/styles/spl.css`.
  The browser demo simulates a service in `src/lib/demoSpl.ts`.
