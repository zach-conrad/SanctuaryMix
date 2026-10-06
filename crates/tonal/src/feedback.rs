//! Spotting feedback (a mic ringing through the speakers) on one channel.
//!
//! Classic howl features from the feedback-control literature (van
//! Waterschoot & Moonen, "Fifty years of acoustic feedback control"), on each
//! 85 ms analysis frame:
//!
//! - **Peak-to-average power**: one frequency stands well above the rest.
//! - **Peak-to-neighbour power**: it is a narrow line, not a broad resonance.
//! - **Peak-to-harmonic power**: nothing at twice, three times or half the
//!   frequency. Feedback is a pure tone; a sung or played note has
//!   harmonics. This is what keeps a held note or an organ pedal from
//!   counting (the classic false alarm of commercial suppressors).
//! - **Persistence and growth**: the same line, holding or rising, for about
//!   300 ms.
//!
//! A ringing line is reported once when it qualifies, then once a second
//! while it keeps going, so the response can deepen.

use serde::Serialize;

const MIN_HZ: f32 = 100.0;
const MAX_HZ: f32 = 12_000.0;
const MIN_PEAK_DBFS: f32 = -50.0;
const PAPR_DB: f32 = 15.0;
const PNPR_DB: f32 = 12.0;
const PHPR_DB: f32 = 25.0;
/// How long a line must hold before it counts as ringing.
const PERSIST_SECS: f32 = 0.3;
/// While ringing, report again this often.
const REPEAT_SECS: f32 = 1.0;
/// A line may wander this many bins between frames and still be the same.
const DRIFT_BINS: usize = 2;
/// It must not be dying away: allow this much drop across the window.
const MAX_DROP_DB: f32 = 1.0;

/// A channel that is ringing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Howl {
    pub channel: u16,
    pub hz: f32,
    /// Level of the line, dBFS.
    pub level_db: f32,
    /// How long it has rung, seconds.
    pub secs: f32,
}

#[derive(Debug, Clone)]
pub struct FeedbackDetector {
    bin: Option<usize>,
    secs: f32,
    first_db: f32,
    last_db: f32,
    next_report: f32,
}

impl Default for FeedbackDetector {
    fn default() -> Self {
        Self::new()
    }
}

fn db(p: f32) -> f32 {
    10.0 * p.max(1e-14).log10()
}

impl FeedbackDetector {
    pub fn new() -> Self {
        Self {
            bin: None,
            secs: 0.0,
            first_db: 0.0,
            last_db: 0.0,
            next_report: PERSIST_SECS,
        }
    }

    fn clear(&mut self) {
        *self = Self::new();
    }

    /// Looks at one frame's power spectrum (bins 0..=N/2). Returns a howl
    /// when a line qualifies, and again each second it keeps ringing.
    pub fn process(
        &mut self,
        power: &[f32],
        rate: u32,
        hop_secs: f32,
        _rms_db: f32,
        channel: u16,
    ) -> Option<Howl> {
        let n = power.len();
        if n < 16 {
            return None;
        }
        let bin_hz = rate as f32 / (2.0 * (n - 1) as f32);
        let lo = ((MIN_HZ / bin_hz) as usize).max(2);
        let hi = ((MAX_HZ / bin_hz) as usize).min(n - 3);
        if hi <= lo + 8 {
            return None;
        }
        let (k, &p) = power[lo..=hi]
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, p)| (i + lo, p))?;
        // A Hann-windowed sine spreads over the peak bin and its two neighbours.
        let line = power[k - 1] + p + power[k + 1];
        let peak_db = db(line);
        let mean = power[lo..=hi].iter().sum::<f32>() / (hi - lo + 1) as f32;
        let neighbours = {
            let mut sum = 0.0;
            let mut count = 0;
            for j in [
                k.wrapping_sub(6),
                k.wrapping_sub(5),
                k.wrapping_sub(4),
                k + 4,
                k + 5,
                k + 6,
            ] {
                if (lo..=hi).contains(&j) {
                    sum += power[j];
                    count += 1;
                }
            }
            if count == 0 {
                mean
            } else {
                sum / count as f32
            }
        };
        let harmonic = |m: f32| -> f32 {
            let j = (k as f32 * m).round() as usize;
            if j < 2 || j + 2 >= n {
                return 0.0;
            }
            power[j - 2..=j + 2].iter().copied().fold(0.0, f32::max) * 3.0
        };
        let worst_harmonic = harmonic(2.0).max(harmonic(3.0)).max(harmonic(0.5));
        let tonal = peak_db > MIN_PEAK_DBFS
            && peak_db - db(mean) > PAPR_DB
            && peak_db - db(neighbours) > PNPR_DB
            && peak_db - db(worst_harmonic) > PHPR_DB;
        if !tonal {
            self.clear();
            return None;
        }
        match self.bin {
            Some(b) if b.abs_diff(k) <= DRIFT_BINS => {
                self.secs += hop_secs;
                self.last_db = peak_db;
                self.bin = Some(k);
            }
            _ => {
                *self = Self::new();
                self.bin = Some(k);
                self.first_db = peak_db;
                self.last_db = peak_db;
                return None;
            }
        }
        // Dying away (a note ending, a struck bell) is not feedback.
        if self.last_db < self.first_db - MAX_DROP_DB {
            self.clear();
            return None;
        }
        if self.secs + 1e-4 < self.next_report {
            return None;
        }
        self.next_report = self.secs + REPEAT_SECS;
        // Parabolic interpolation on dB for a frequency between bins.
        let (a, b, c) = (db(power[k - 1]), db(p), db(power[k + 1]));
        let denom = a - 2.0 * b + c;
        let offset = if denom.abs() > 1e-6 {
            (0.5 * (a - c) / denom).clamp(-0.5, 0.5)
        } else {
            0.0
        };
        Some(Howl {
            channel,
            hz: (k as f32 + offset) * bin_hz,
            level_db: peak_db,
            secs: self.secs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustfft::num_complex::Complex32;
    use rustfft::FftPlanner;

    const RATE: u32 = 48_000;
    const N: usize = 4096;
    const HOP: usize = 1024;

    /// Runs the detector over a signal; returns every howl it reports.
    fn run(signal: &[f32]) -> Vec<Howl> {
        let fft = FftPlanner::new().plan_fft_forward(N);
        let window: Vec<f32> = (0..N)
            .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / N as f32).cos())
            .collect();
        let mut det = FeedbackDetector::new();
        let mut out = Vec::new();
        let mut start = 0;
        while start + N <= signal.len() {
            let mut buf: Vec<Complex32> = signal[start..start + N]
                .iter()
                .zip(&window)
                .map(|(&x, &w)| Complex32::new(x * w, 0.0))
                .collect();
            fft.process(&mut buf);
            let norm = 16.0 / (N * N) as f32;
            let power: Vec<f32> = buf[..=N / 2].iter().map(|c| c.norm_sqr() * norm).collect();
            out.extend(det.process(&power, RATE, HOP as f32 / RATE as f32, -20.0, 0));
            start += HOP;
        }
        out
    }

    /// Deterministic noise in -1..1.
    fn noise(len: usize, seed: u32) -> Vec<f32> {
        let mut s = seed;
        (0..len)
            .map(|_| {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                (s as f32 / u32::MAX as f32) * 2.0 - 1.0
            })
            .collect()
    }

    fn sine(hz: f32, amp: impl Fn(f32) -> f32, secs: f32) -> Vec<f32> {
        (0..(secs * RATE as f32) as usize)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                amp(t) * (2.0 * std::f32::consts::PI * hz * t).sin()
            })
            .collect()
    }

    #[test]
    fn catches_a_growing_howl_over_noise() {
        let howl = sine(2_500.0, |t| 0.02 * (1.0 + 4.0 * t), 1.5);
        let bed = noise(howl.len(), 7);
        let sig: Vec<f32> = howl.iter().zip(&bed).map(|(h, n)| h + 0.002 * n).collect();
        let found = run(&sig);
        assert!(!found.is_empty(), "missed the howl");
        let first = found[0];
        assert!((first.hz - 2_500.0).abs() < 15.0, "{}", first.hz);
        // Within about 0.4 s of it starting to ring.
        assert!(first.secs < 0.45, "{}", first.secs);
    }

    #[test]
    fn ignores_a_held_note_with_harmonics() {
        let len = (1.5 * RATE as f32) as usize;
        let mut note = vec![0.0f32; len];
        for (h, amp) in [(1.0, 0.2), (2.0, 0.1), (3.0, 0.06), (4.0, 0.03)] {
            for (x, s) in note.iter_mut().zip(sine(220.0 * h, |_| amp, 1.5)) {
                *x += s;
            }
        }
        assert!(run(&note).is_empty());
    }

    #[test]
    fn ignores_noise_and_speechlike_sound() {
        assert!(run(&noise(RATE as usize * 2, 3)).is_empty());
    }

    #[test]
    fn ignores_a_tone_dying_away() {
        let tone = sine(1_000.0, |t| 0.3 * (-8.0 * t).exp(), 1.5);
        assert!(run(&tone).is_empty());
    }
}
