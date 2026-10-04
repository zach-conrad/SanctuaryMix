//! Lock-free peak/RMS accumulation, safe to call from the audio thread.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use mix_core::level::linear_to_db;
use mix_core::{ChannelMeter, MeterFrame};

/// Samples at or above this magnitude count as clipped.
const CLIP_THRESHOLD: f32 = 0.999;

struct Accumulator {
    /// Max |sample| as f32 bits. Non-negative floats order the same as their bits.
    peak: AtomicU32,
    /// Sum of squares as f64 bits.
    sum_sq: AtomicU64,
    count: AtomicU64,
    clipped: AtomicBool,
}

pub struct MeterBank {
    channels: Vec<Accumulator>,
}

impl MeterBank {
    pub fn new(channels: usize) -> Self {
        Self {
            channels: (0..channels)
                .map(|_| Accumulator {
                    peak: AtomicU32::new(0),
                    sum_sq: AtomicU64::new(0),
                    count: AtomicU64::new(0),
                    clipped: AtomicBool::new(false),
                })
                .collect(),
        }
    }

    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    /// Accumulates one interleaved buffer. Called from the audio callback.
    pub fn push_interleaved(&self, data: &[f32]) {
        let n = self.channels.len();
        if n == 0 {
            return;
        }
        let frames = data.len() / n;
        for (ch, acc) in self.channels.iter().enumerate() {
            let mut peak = 0f32;
            let mut sum_sq = 0f64;
            for frame in 0..frames {
                let s = data[frame * n + ch];
                peak = peak.max(s.abs());
                sum_sq += (s as f64) * (s as f64);
            }
            acc.peak.fetch_max(peak.to_bits(), Ordering::Relaxed);
            // CAS loop rather than fetch_update, which newer Rust renames.
            let mut bits = acc.sum_sq.load(Ordering::Relaxed);
            loop {
                let next = (f64::from_bits(bits) + sum_sq).to_bits();
                match acc.sum_sq.compare_exchange_weak(
                    bits,
                    next,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => break,
                    Err(current) => bits = current,
                }
            }
            acc.count.fetch_add(frames as u64, Ordering::Relaxed);
            if peak >= CLIP_THRESHOLD {
                acc.clipped.store(true, Ordering::Relaxed);
            }
        }
    }

    /// Reads and clears everything accumulated since the last call.
    pub fn take_frame(&self, sample_rate: u32) -> MeterFrame {
        let channels = self
            .channels
            .iter()
            .enumerate()
            .map(|(i, acc)| {
                let peak = f32::from_bits(acc.peak.swap(0, Ordering::Relaxed));
                let sum_sq = f64::from_bits(acc.sum_sq.swap(0, Ordering::Relaxed));
                let count = acc.count.swap(0, Ordering::Relaxed);
                let rms = if count == 0 {
                    0.0
                } else {
                    (sum_sq / count as f64).sqrt() as f32
                };
                ChannelMeter {
                    channel: i as u16,
                    peak_db: linear_to_db(peak),
                    rms_db: linear_to_db(rms),
                    clipped: acc.clipped.swap(false, Ordering::Relaxed),
                }
            })
            .collect();
        MeterFrame {
            sample_rate,
            channels,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mix_core::level::SILENCE_DB;

    #[test]
    fn measures_each_interleaved_channel() {
        let bank = MeterBank::new(2);
        // ch0: constant 0.5, ch1: silence
        let buf: Vec<f32> = (0..256).flat_map(|_| [0.5, 0.0]).collect();
        bank.push_interleaved(&buf);
        let frame = bank.take_frame(48_000);
        let m0 = frame.channels[0];
        assert!((m0.peak_db - -6.0206).abs() < 0.01);
        assert!((m0.rms_db - -6.0206).abs() < 0.01);
        assert!(!m0.clipped);
        assert_eq!(frame.channels[1].peak_db, SILENCE_DB);
    }

    #[test]
    fn sine_rms_is_3db_below_peak() {
        let bank = MeterBank::new(1);
        let buf: Vec<f32> = (0..4800).map(|i| (i as f32 * 0.1).sin()).collect();
        bank.push_interleaved(&buf);
        let m = bank.take_frame(48_000).channels[0];
        assert!((m.peak_db - m.rms_db - 3.01).abs() < 0.1, "{m:?}");
    }

    #[test]
    fn take_resets_and_flags_clips() {
        let bank = MeterBank::new(1);
        bank.push_interleaved(&[1.0, -0.2]);
        assert!(bank.take_frame(48_000).channels[0].clipped);
        let after = bank.take_frame(48_000).channels[0];
        assert!(!after.clipped);
        assert_eq!(after.peak_db, SILENCE_DB);
    }
}
