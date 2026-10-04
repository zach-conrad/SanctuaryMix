//! Streaming conversion from the interface's sample rate to the models' 16 kHz.
//!
//! A windowed-sinc low-pass evaluated at each output instant. It works for any
//! input rate (44.1, 48, 96 kHz) and costs about 50 multiply-adds per output
//! sample at 48 kHz, which is nothing next to the models.

use std::f64::consts::PI;

pub const MODEL_RATE: u32 = 16_000;

/// Keep everything below this; the models only look up to 7.5 kHz.
const CUTOFF_HZ: f64 = 7_200.0;
/// Kernel half-width in output samples.
const HALF_WIDTH_OUT: f64 = 8.0;

pub struct Resampler {
    /// Input samples per output sample.
    step: f64,
    /// Normalised cutoff (cycles per input sample).
    cutoff: f64,
    /// Kernel half-width in input samples.
    half: usize,
    history: Vec<f32>,
    /// Absolute input index of `history[0]`.
    base: u64,
    /// Absolute input position of the next output sample.
    next: f64,
}

impl Resampler {
    pub fn new(input_rate: u32) -> Self {
        let step = input_rate as f64 / MODEL_RATE as f64;
        Self {
            step,
            cutoff: CUTOFF_HZ / input_rate as f64,
            half: (HALF_WIDTH_OUT * step).ceil() as usize,
            history: Vec::new(),
            base: 0,
            next: 0.0,
        }
    }

    fn passthrough(&self) -> bool {
        (self.step - 1.0).abs() < 1e-9
    }

    fn kernel(&self, t: f64) -> f64 {
        let w = self.half as f64;
        if t.abs() >= w {
            return 0.0;
        }
        let x = 2.0 * self.cutoff * t;
        let sinc = if x.abs() < 1e-9 {
            1.0
        } else {
            (PI * x).sin() / (PI * x)
        };
        let window = 0.5 + 0.5 * (PI * t / w).cos();
        2.0 * self.cutoff * sinc * window
    }

    /// Feeds input samples and appends whatever 16 kHz output is ready to `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if self.passthrough() {
            out.extend_from_slice(input);
            return;
        }
        self.history.extend_from_slice(input);
        let end = self.base + self.history.len() as u64;
        let half = self.half as i64;
        while (self.next.floor() as i64 + half) < end as i64 {
            let centre = self.next.floor() as i64;
            let mut acc = 0.0f64;
            for i in (centre - half + 1)..=(centre + half) {
                if i < self.base as i64 {
                    continue;
                }
                let s = self.history[(i as u64 - self.base) as usize] as f64;
                acc += s * self.kernel(self.next - i as f64);
            }
            out.push(acc as f32);
            self.next += self.step;
        }
        // Drop input no future output can reach.
        let keep_from = (self.next.floor() as i64 - half).max(self.base as i64) as u64;
        let drop = (keep_from - self.base) as usize;
        if drop > 0 {
            self.history.drain(..drop);
            self.base = keep_from;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, hz: f64, secs: f64) -> Vec<f32> {
        (0..(rate as f64 * secs) as usize)
            .map(|n| (2.0 * PI * hz * n as f64 / rate as f64).sin() as f32 * 0.5)
            .collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt()
    }

    fn run(rate: u32, input: &[f32], block: usize) -> Vec<f32> {
        let mut r = Resampler::new(rate);
        let mut out = Vec::new();
        for chunk in input.chunks(block) {
            r.process(chunk, &mut out);
        }
        out
    }

    #[test]
    fn keeps_speech_band_tones_at_the_right_level() {
        for rate in [44_100, 48_000, 96_000] {
            let out = run(rate, &tone(rate, 1_000.0, 1.0), 256);
            assert!(
                (out.len() as i64 - 16_000).abs() < 300,
                "{rate}: {}",
                out.len()
            );
            let steady = &out[1_000..out.len() - 1_000];
            assert!(
                (rms(steady) - 0.3536).abs() < 0.01,
                "{rate}: {}",
                rms(steady)
            );
        }
    }

    #[test]
    fn removes_what_would_alias() {
        // 12 kHz would fold back to 4 kHz at 16 kHz; it must be filtered out.
        let out = run(48_000, &tone(48_000, 12_000.0, 1.0), 480);
        assert!(rms(&out[1_000..]) < 0.01, "{}", rms(&out[1_000..]));
    }

    #[test]
    fn block_size_does_not_change_the_result() {
        let input = tone(48_000, 440.0, 0.5);
        let a = run(48_000, &input, 64);
        let b = run(48_000, &input, 1_000);
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(&b) {
            assert!((x - y).abs() < 1e-6);
        }
    }

    #[test]
    fn passes_16k_straight_through() {
        let input = tone(16_000, 440.0, 0.1);
        assert_eq!(run(16_000, &input, 100), input);
    }
}
