//! YAMNet's input features: a log-mel spectrogram, computed exactly as
//! Google's `features.py` does (25 ms periodic Hann window, 10 ms hop, 512-point
//! FFT magnitude, 64 HTK mel bands from 125 Hz to 7.5 kHz, `ln(mel + 0.001)`).
//! `tests/golden.rs` checks it against TensorFlow's output.

use std::sync::Arc;

use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};

pub const WINDOW: usize = 400;
pub const HOP: usize = 160;
pub const FFT_LEN: usize = 512;
pub const BINS: usize = FFT_LEN / 2 + 1;
pub const MEL_BANDS: usize = 64;
const MEL_MIN_HZ: f64 = 125.0;
const MEL_MAX_HZ: f64 = 7_500.0;
const LOG_OFFSET: f32 = 0.001;
const SAMPLE_RATE: f64 = 16_000.0;

fn hz_to_mel(hz: f64) -> f64 {
    1127.0 * (1.0 + hz / 700.0).ln()
}

/// `tf.signal.linear_to_mel_weight_matrix`, as `[bin][band]`.
fn mel_matrix() -> Vec<[f32; MEL_BANDS]> {
    let edges: Vec<f64> = (0..MEL_BANDS + 2)
        .map(|i| {
            let lo = hz_to_mel(MEL_MIN_HZ);
            let hi = hz_to_mel(MEL_MAX_HZ);
            lo + (hi - lo) * i as f64 / (MEL_BANDS + 1) as f64
        })
        .collect();
    let nyquist = SAMPLE_RATE / 2.0;
    (0..BINS)
        .map(|bin| {
            let mut row = [0f32; MEL_BANDS];
            // TensorFlow leaves the DC bin out.
            if bin == 0 {
                return row;
            }
            let mel = hz_to_mel(nyquist * bin as f64 / (BINS - 1) as f64);
            for (band, w) in row.iter_mut().enumerate() {
                let (lower, centre, upper) = (edges[band], edges[band + 1], edges[band + 2]);
                let rising = (mel - lower) / (centre - lower);
                let falling = (upper - mel) / (upper - centre);
                *w = rising.min(falling).max(0.0) as f32;
            }
            row
        })
        .collect()
}

/// Turns 16 kHz audio into log-mel frames, 100 per second.
pub struct LogMel {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    mel: Vec<[f32; MEL_BANDS]>,
    pending: Vec<f32>,
    scratch: Vec<Complex32>,
}

impl Default for LogMel {
    fn default() -> Self {
        Self::new()
    }
}

impl LogMel {
    pub fn new() -> Self {
        let window = (0..WINDOW)
            .map(|n| {
                (0.5 - 0.5 * (2.0 * std::f64::consts::PI * n as f64 / WINDOW as f64).cos()) as f32
            })
            .collect();
        Self {
            fft: FftPlanner::new().plan_fft_forward(FFT_LEN),
            window,
            mel: mel_matrix(),
            pending: Vec::with_capacity(WINDOW + HOP * 4),
            scratch: vec![Complex32::default(); FFT_LEN],
        }
    }

    /// Feeds 16 kHz samples and calls `on_frame` for each finished frame.
    pub fn process(&mut self, input: &[f32], mut on_frame: impl FnMut(&[f32; MEL_BANDS])) {
        self.pending.extend_from_slice(input);
        let mut start = 0;
        while start + WINDOW <= self.pending.len() {
            let frame = self.frame(start);
            on_frame(&frame);
            start += HOP;
        }
        self.pending.drain(..start);
    }

    fn frame(&mut self, start: usize) -> [f32; MEL_BANDS] {
        for (i, c) in self.scratch.iter_mut().enumerate() {
            let s = if i < WINDOW {
                self.pending[start + i] * self.window[i]
            } else {
                0.0
            };
            *c = Complex32::new(s, 0.0);
        }
        self.fft.process(&mut self.scratch);
        let mut bands = [0f32; MEL_BANDS];
        for (bin, weights) in self.mel.iter().enumerate() {
            let mag = self.scratch[bin].norm();
            if mag == 0.0 {
                continue;
            }
            for (b, w) in bands.iter_mut().zip(weights) {
                *b += mag * w;
            }
        }
        bands.map(|b| (b + LOG_OFFSET).ln())
    }
}
