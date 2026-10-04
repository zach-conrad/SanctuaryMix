//! A and C weighted sound level metering (IEC 61672 style) for one input.
//!
//! [`SplMeter`] is plain DSP with no threads or locks: feed it audio from the
//! capture thread (never the real-time callback) and read [`SplReading`]s and
//! one-second history whenever you like. Levels are kept as raw mean squares
//! in dBFS terms, and the calibration offset is applied only when they are
//! read, so recalibrating shifts the whole history at once.

use std::collections::VecDeque;
use std::f64::consts::PI;

use mix_core::spl::{AcLevel, SplConfig, SplPoint, SplReading, Weighting};

/// Two hours of one-second points: a long service with room to spare.
pub const HISTORY_SECS: usize = 2 * 60 * 60;

const FAST_SECS: f64 = 0.125;
const SLOW_SECS: f64 = 1.0;
/// Mean squares below this read as silence (−140 dBFS) rather than −∞.
const FLOOR_MS: f64 = 1e-14;
/// Calibration needs a real signal; anything quieter is almost certainly the wrong input.
const CALIBRATION_MIN_DBFS: f32 = -80.0;

// Analog pole frequencies from IEC 61672-1, in Hz.
const F1: f64 = 20.598_997;
const F2: f64 = 107.652_65;
const F3: f64 = 737.862_23;
const F4: f64 = 12_194.217;

/// Transposed direct form II biquad in f64, so low-frequency poles stay stable.
#[derive(Debug, Clone)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    z: [f64; 2],
}

impl Biquad {
    /// Bilinear transform of `(b2 s² + b1 s + b0) / (a2 s² + a1 s + a0)`.
    fn bilinear(num: [f64; 3], den: [f64; 3], sample_rate: f64) -> Self {
        let k = 2.0 * sample_rate;
        let k2 = k * k;
        let [b2, b1, b0] = num;
        let [a2, a1, a0] = den;
        let norm = a2 * k2 + a1 * k + a0;
        Self {
            b: [
                (b2 * k2 + b1 * k + b0) / norm,
                (2.0 * b0 - 2.0 * b2 * k2) / norm,
                (b2 * k2 - b1 * k + b0) / norm,
            ],
            a: [
                (2.0 * a0 - 2.0 * a2 * k2) / norm,
                (a2 * k2 - a1 * k + a0) / norm,
            ],
            z: [0.0; 2],
        }
    }

    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }

    /// Complex response at `freq`, as (re, im).
    fn response(&self, freq: f64, sample_rate: f64) -> (f64, f64) {
        let w = 2.0 * PI * freq / sample_rate;
        // z^-1 = e^{-jw}
        let (c1, s1) = (w.cos(), -w.sin());
        let (c2, s2) = ((2.0 * w).cos(), -(2.0 * w).sin());
        let num = (
            self.b[0] + self.b[1] * c1 + self.b[2] * c2,
            self.b[1] * s1 + self.b[2] * s2,
        );
        let den = (
            1.0 + self.a[0] * c1 + self.a[1] * c2,
            self.a[0] * s1 + self.a[1] * s2,
        );
        let d = den.0 * den.0 + den.1 * den.1;
        (
            (num.0 * den.0 + num.1 * den.1) / d,
            (num.1 * den.0 - num.0 * den.1) / d,
        )
    }
}

/// A cascade of biquads with a gain that makes 1 kHz exactly 0 dB.
#[derive(Debug, Clone)]
pub struct WeightingFilter {
    sections: Vec<Biquad>,
    gain: f64,
    sample_rate: f64,
}

impl WeightingFilter {
    pub fn new(weighting: Weighting, sample_rate: u32) -> Self {
        let fs = sample_rate as f64;
        let w = |f: f64| 2.0 * PI * f;
        // The 12.2 kHz pole is pre-warped so the bilinear transform puts it
        // where it belongs; the others are far enough below Nyquist not to need it.
        // Below about 30 kHz sampling the pole is near or past Nyquist and
        // pre-warping would push it to infinity, so it is left as is.
        let half_angle = w(F4) / (2.0 * fs);
        let w4 = if half_angle < 1.2 {
            2.0 * fs * half_angle.tan()
        } else {
            w(F4)
        };
        let (w1, w2, w3) = (w(F1), w(F2), w(F3));
        // Double zero at DC with a double pole at 20.6 Hz.
        let high_pass = Biquad::bilinear([1.0, 0.0, 0.0], [1.0, 2.0 * w1, w1 * w1], fs);
        // Double pole at 12.2 kHz.
        let low_pass = Biquad::bilinear([0.0, 0.0, 1.0], [1.0, 2.0 * w4, w4 * w4], fs);
        let mut sections = vec![high_pass, low_pass];
        if weighting == Weighting::A {
            // Two more zeros at DC with poles at 107.7 Hz and 737.9 Hz.
            sections.push(Biquad::bilinear(
                [1.0, 0.0, 0.0],
                [1.0, w2 + w3, w2 * w3],
                fs,
            ));
        }
        let mut filter = Self {
            sections,
            gain: 1.0,
            sample_rate: fs,
        };
        filter.gain = 1.0 / filter.magnitude(1_000.0);
        filter
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        self.sections
            .iter_mut()
            .fold(x * self.gain, |acc, s| s.process(acc))
    }

    /// |H(freq)| including the 1 kHz normalisation.
    pub fn magnitude(&self, freq: f64) -> f64 {
        let (mut re, mut im) = (self.gain, 0.0);
        for s in &self.sections {
            let (r, i) = s.response(freq, self.sample_rate);
            (re, im) = (re * r - im * i, re * i + im * r);
        }
        (re * re + im * im).sqrt()
    }
}

/// One weighting's running state.
#[derive(Debug, Clone)]
struct Channel {
    filter: WeightingFilter,
    fast: f64,
    slow: f64,
    second: f64,
    total: f64,
}

impl Channel {
    fn new(weighting: Weighting, sample_rate: u32) -> Self {
        Self {
            filter: WeightingFilter::new(weighting, sample_rate),
            fast: 0.0,
            slow: 0.0,
            second: 0.0,
            total: 0.0,
        }
    }
}

/// One stored second: mean squares, before the offset.
#[derive(Debug, Clone, Copy)]
struct RawPoint {
    t: f64,
    a: f64,
    c: f64,
}

/// Measures one mono signal: fast/slow levels, Leq windows, maximum and peak.
#[derive(Debug, Clone)]
pub struct SplMeter {
    sample_rate: u32,
    fast_coeff: f64,
    slow_coeff: f64,
    a: Channel,
    c: Channel,
    /// Samples in the current (unfinished) second.
    second_count: u32,
    total_count: u64,
    /// Highest fast A mean square since the reset.
    a_max: f64,
    /// Highest |C-weighted sample| since the reset.
    c_peak: f64,
    history: VecDeque<RawPoint>,
}

impl SplMeter {
    pub fn new(sample_rate: u32) -> Self {
        let fs = sample_rate.max(1) as f64;
        Self {
            sample_rate,
            fast_coeff: 1.0 - (-1.0 / (FAST_SECS * fs)).exp(),
            slow_coeff: 1.0 - (-1.0 / (SLOW_SECS * fs)).exp(),
            a: Channel::new(Weighting::A, sample_rate),
            c: Channel::new(Weighting::C, sample_rate),
            second_count: 0,
            total_count: 0,
            a_max: 0.0,
            c_peak: 0.0,
            history: VecDeque::with_capacity(HISTORY_SECS),
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Starts over: clears history, Leq, maximum and peak.
    pub fn reset(&mut self) {
        *self = Self::new(self.sample_rate);
    }

    /// Measures one channel out of an interleaved buffer.
    pub fn process_interleaved(&mut self, data: &[f32], channels: usize, channel: usize) {
        if channels == 0 || channel >= channels {
            return;
        }
        for frame in data.chunks_exact(channels) {
            self.process_sample(frame[channel]);
        }
    }

    pub fn process(&mut self, samples: &[f32]) {
        for &s in samples {
            self.process_sample(s);
        }
    }

    #[inline]
    fn process_sample(&mut self, sample: f32) {
        let x = sample as f64;
        let ya = self.a.filter.process(x);
        let yc = self.c.filter.process(x);
        let (sa, sc) = (ya * ya, yc * yc);
        for (ch, sq) in [(&mut self.a, sa), (&mut self.c, sc)] {
            ch.fast += (sq - ch.fast) * self.fast_coeff;
            ch.slow += (sq - ch.slow) * self.slow_coeff;
            ch.second += sq;
            ch.total += sq;
        }
        self.a_max = self.a_max.max(self.a.fast);
        self.c_peak = self.c_peak.max(yc.abs());
        self.second_count += 1;
        self.total_count += 1;
        if self.second_count >= self.sample_rate {
            self.close_second();
        }
    }

    fn close_second(&mut self) {
        let n = self.second_count.max(1) as f64;
        if self.history.len() == HISTORY_SECS {
            self.history.pop_front();
        }
        self.history.push_back(RawPoint {
            t: self.total_count as f64 / self.sample_rate as f64,
            a: self.a.second / n,
            c: self.c.second / n,
        });
        self.a.second = 0.0;
        self.c.second = 0.0;
        self.second_count = 0;
    }

    /// Mean squares over the last `secs` seconds, counting the unfinished one.
    fn leq_ms(&self, secs: usize) -> (f64, f64) {
        let partial = self.second_count as f64 / self.sample_rate as f64;
        let (mut a, mut c, mut weight) = (self.a.second, self.c.second, self.second_count as f64);
        // Partial-second sums are in samples; whole seconds are already means.
        let fs = self.sample_rate as f64;
        let whole = secs.saturating_sub(partial.ceil() as usize).max(1);
        for p in self.history.iter().rev().take(whole) {
            a += p.a * fs;
            c += p.c * fs;
            weight += fs;
        }
        if weight == 0.0 {
            return (0.0, 0.0);
        }
        (a / weight, c / weight)
    }

    /// Fast or slow level in dBFS, before the calibration offset.
    pub fn level_dbfs(&self, weighting: Weighting, slow: bool) -> f32 {
        let ch = match weighting {
            Weighting::A => &self.a,
            Weighting::C => &self.c,
        };
        ms_to_db(if slow { ch.slow } else { ch.fast }, 0.0)
    }

    /// The offset that makes the current slow level read `reference_db`, or
    /// `None` if the input is too quiet to calibrate against.
    pub fn calibration_offset(&self, weighting: Weighting, reference_db: f32) -> Option<f32> {
        let measured = self.level_dbfs(weighting, true);
        (measured >= CALIBRATION_MIN_DBFS).then_some(reference_db - measured)
    }

    /// Everything measured so far, in dB SPL per `config`. `None` before any audio.
    pub fn reading(&self, config: &SplConfig, source: u16) -> Option<SplReading> {
        if self.total_count == 0 {
            return None;
        }
        let off = config.offset_db;
        let pair = |a: f64, c: f64| AcLevel {
            a: ms_to_db(a, off),
            c: ms_to_db(c, off),
        };
        let (a1, c1) = self.leq_ms(60);
        let (a15, c15) = self.leq_ms(15 * 60);
        let n = self.total_count as f64;
        Some(SplReading {
            source,
            calibrated: config.calibrated,
            fast: pair(self.a.fast, self.c.fast),
            slow: pair(self.a.slow, self.c.slow),
            leq_1m: pair(a1, c1),
            leq_15m: pair(a15, c15),
            leq_total: pair(self.a.total / n, self.c.total / n),
            a_max: ms_to_db(self.a_max, off),
            c_peak: ms_to_db(self.c_peak * self.c_peak, off),
            seconds: n / self.sample_rate as f64,
        })
    }

    /// One-second Leq points for the last `secs` seconds, oldest first.
    pub fn history(&self, config: &SplConfig, secs: usize) -> Vec<SplPoint> {
        let skip = self.history.len().saturating_sub(secs);
        self.history
            .iter()
            .skip(skip)
            .map(|p| SplPoint {
                t: p.t,
                a: ms_to_db(p.a, config.offset_db),
                c: ms_to_db(p.c, config.offset_db),
            })
            .collect()
    }
}

fn ms_to_db(ms: f64, offset: f32) -> f32 {
    (10.0 * ms.max(FLOOR_MS).log10()) as f32 + offset
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db(x: f64) -> f64 {
        20.0 * x.log10()
    }

    /// IEC 61672-1 Table 3 design goals (Hz, A dB, C dB).
    const TABLE: &[(f64, f64, f64)] = &[
        (31.5, -39.4, -3.0),
        (63.0, -26.2, -0.8),
        (125.0, -16.1, -0.2),
        (250.0, -8.6, 0.0),
        (500.0, -3.2, 0.0),
        (1_000.0, 0.0, 0.0),
        (2_000.0, 1.2, -0.2),
        (4_000.0, 1.0, -0.8),
        (8_000.0, -1.1, -3.0),
        (10_000.0, -2.5, -4.4),
        (12_500.0, -4.3, -6.2),
    ];

    #[test]
    fn weighting_curves_match_the_standard() {
        for rate in [44_100, 48_000, 96_000] {
            let a = WeightingFilter::new(Weighting::A, rate);
            let c = WeightingFilter::new(Weighting::C, rate);
            for &(f, want_a, want_c) in TABLE {
                // Pre-warping leaves under 0.9 dB of error up top at 44.1 kHz,
                // inside the Class 1 tolerance (+1.5 dB at 8 kHz and up).
                let tol = match f {
                    f if f >= 8_000.0 => 1.0,
                    f if f >= 4_000.0 => 0.5,
                    _ => 0.3,
                };
                let got_a = db(a.magnitude(f));
                let got_c = db(c.magnitude(f));
                assert!(
                    (got_a - want_a).abs() < tol,
                    "A {f} Hz @ {rate}: {got_a:.2}"
                );
                assert!(
                    (got_c - want_c).abs() < tol,
                    "C {f} Hz @ {rate}: {got_c:.2}"
                );
            }
        }
    }

    fn sine(freq: f64, amp: f64, rate: u32, secs: f64) -> Vec<f32> {
        let n = (rate as f64 * secs) as usize;
        (0..n)
            .map(|i| (amp * (2.0 * PI * freq * i as f64 / rate as f64).sin()) as f32)
            .collect()
    }

    #[test]
    fn full_scale_1k_sine_reads_minus_3_dbfs_plus_offset() {
        let mut m = SplMeter::new(48_000);
        m.process(&sine(1_000.0, 1.0, 48_000, 8.0));
        let config = SplConfig {
            source: Some(0),
            offset_db: 100.0,
            calibrated: true,
        };
        let r = m.reading(&config, 0).unwrap();
        for v in [
            r.fast.a, r.fast.c, r.slow.a, r.slow.c, r.leq_1m.a, r.leq_1m.c,
        ] {
            assert!((v - 96.99).abs() < 0.15, "{r:?}");
        }
        assert!((r.leq_total.a - 96.99).abs() < 0.3, "{r:?}");
        // Peak of a 1 kHz sine is 3 dB over its RMS.
        assert!((r.c_peak - 100.0).abs() < 0.5, "{r:?}");
        assert!(r.calibrated);
        assert_eq!(m.history(&config, 60).len(), 8);
    }

    #[test]
    fn low_frequency_reads_lower_on_a_than_c() {
        let mut m = SplMeter::new(48_000);
        m.process(&sine(63.0, 0.5, 48_000, 2.0));
        let r = m.reading(&SplConfig::default(), 0).unwrap();
        let gap = r.leq_1m.c - r.leq_1m.a;
        assert!((gap - 25.4).abs() < 0.5, "{r:?}");
    }

    #[test]
    fn leq_windows_average_energy_not_decibels() {
        let mut m = SplMeter::new(48_000);
        m.process(&sine(1_000.0, 1.0, 48_000, 30.0));
        m.process(&vec![0.0; 48_000 * 30]);
        let r = m.reading(&SplConfig::default(), 0).unwrap();
        // Half the minute at −3 dBFS, half silent: 3 dB below the loud half.
        let loud = -3.01 + SplConfig::DEFAULT_OFFSET_DB;
        assert!((r.leq_1m.a - (loud - 3.01)).abs() < 0.2, "{r:?}");
        assert!((r.a_max - loud).abs() < 0.2, "{r:?}");
        assert!(r.fast.a < loud - 60.0, "{r:?}");
    }

    #[test]
    fn calibration_offset_makes_slow_level_read_the_reference() {
        let mut m = SplMeter::new(48_000);
        m.process(&sine(1_000.0, 0.1, 48_000, 4.0));
        let off = m.calibration_offset(Weighting::C, 94.0).unwrap();
        let config = SplConfig {
            source: Some(3),
            offset_db: off,
            calibrated: true,
        };
        let r = m.reading(&config, 3).unwrap();
        assert!((r.slow.c - 94.0).abs() < 0.01, "{r:?}");

        let quiet = SplMeter::new(48_000);
        assert_eq!(quiet.calibration_offset(Weighting::A, 94.0), None);
    }

    #[test]
    fn interleaved_reads_only_its_channel_and_history_is_bounded() {
        let mut m = SplMeter::new(100);
        let buf: Vec<f32> = (0..100).flat_map(|_| [0.0, 0.5, 0.0]).collect();
        m.process_interleaved(&buf, 3, 0);
        let r = m.reading(&SplConfig::default(), 0).unwrap();
        assert!(r.leq_total.c < -100.0 + SplConfig::DEFAULT_OFFSET_DB);
        for _ in 0..HISTORY_SECS + 5 {
            m.process(&[0.0; 100]);
        }
        assert_eq!(
            m.history(&SplConfig::default(), usize::MAX).len(),
            HISTORY_SECS
        );
        m.reset();
        assert!(m.reading(&SplConfig::default(), 0).is_none());
    }
}
