//! Guided calibration: listens to pink noise or a calibrator on the
//! measurement input until the level has held steady for a few seconds, then
//! averages that stretch. Refuses clipped or near-silent input, so a bad
//! reading can't quietly become the offset.

use std::collections::VecDeque;

use mix_core::spl::Weighting;

use super::{ms_to_db, WeightingFilter};

/// Seconds of steady signal to average.
pub const STEADY_SECS: f32 = 5.0;
/// Give up after this long without a steady stretch.
pub const TIMEOUT_SECS: f32 = 30.0;
const CHUNK_SECS: f64 = 0.25;
/// Let the weighting filter settle before trusting anything.
const SETTLE_CHUNKS: usize = 2;
/// Pink noise measured in quarter seconds wanders well inside this.
const MAX_SPREAD_DB: f32 = 1.5;
/// Quieter than this, the noise floor and hum start to count.
const MIN_DBFS: f32 = -65.0;
/// Raw samples at or over this mean the converter is clipping.
const CLIP: f32 = 0.98;

pub const TOO_QUIET: &str =
    "Too quiet to calibrate. Play pink noise through the system or fit the calibrator on the mic.";
pub const CLIPPING: &str =
    "The measurement input is clipping. Turn its gain down a little, then calibrate again.";
pub const UNSTEADY: &str =
    "Waiting for the level to settle. Keep the noise steady and the room quiet.";

#[derive(Debug, Clone, PartialEq)]
pub enum CalibrationProgress {
    Listening {
        level_dbfs: f32,
        steady_secs: f32,
        elapsed_secs: f32,
        /// Why the steady count isn't growing, if it isn't.
        hold: Option<&'static str>,
    },
    /// The averaged weighted level over the steady stretch.
    Done {
        measured_dbfs: f32,
    },
    Failed {
        reason: String,
    },
}

#[derive(Debug, Clone)]
pub struct Calibrator {
    filter: WeightingFilter,
    chunk_len: usize,
    sum: f64,
    n: usize,
    peak: f32,
    /// Mean squares of the current steady run, one per chunk.
    run: VecDeque<f64>,
    chunks: usize,
    last_level: f32,
    last_hold: Option<&'static str>,
    result: Option<CalibrationProgress>,
}

impl Calibrator {
    pub fn new(weighting: Weighting, sample_rate: u32) -> Self {
        Self {
            filter: WeightingFilter::new(weighting, sample_rate),
            chunk_len: ((sample_rate as f64 * CHUNK_SECS) as usize).max(1),
            sum: 0.0,
            n: 0,
            peak: 0.0,
            run: VecDeque::new(),
            chunks: 0,
            last_level: f32::NEG_INFINITY,
            last_hold: None,
            result: None,
        }
    }

    fn needed_chunks() -> usize {
        (STEADY_SECS as f64 / CHUNK_SECS).round() as usize
    }

    /// Measures one channel out of an interleaved buffer.
    pub fn feed_interleaved(&mut self, data: &[f32], channels: usize, channel: usize) {
        if channels == 0 || channel >= channels {
            return;
        }
        for frame in data.chunks_exact(channels) {
            self.feed_sample(frame[channel]);
        }
    }

    pub fn feed(&mut self, samples: &[f32]) {
        for &s in samples {
            self.feed_sample(s);
        }
    }

    fn feed_sample(&mut self, sample: f32) {
        if self.result.is_some() {
            return;
        }
        let y = self.filter.process(sample as f64);
        self.sum += y * y;
        self.n += 1;
        self.peak = self.peak.max(sample.abs());
        if self.n >= self.chunk_len {
            self.close_chunk();
        }
    }

    fn close_chunk(&mut self) {
        let ms = self.sum / self.n as f64;
        let peak = self.peak;
        (self.sum, self.n, self.peak) = (0.0, 0, 0.0);
        self.chunks += 1;
        self.last_level = ms_to_db(ms, 0.0);
        if self.chunks <= SETTLE_CHUNKS {
            return;
        }

        self.last_hold = if peak >= CLIP {
            Some(CLIPPING)
        } else if self.last_level < MIN_DBFS {
            Some(TOO_QUIET)
        } else {
            None
        };
        if self.last_hold.is_some() {
            self.run.clear();
        } else {
            self.run.push_back(ms);
            // Keep only the trailing stretch that stays within the spread.
            while spread_db(&self.run) > MAX_SPREAD_DB {
                self.run.pop_front();
            }
            if self.run.len()
                < self
                    .chunks
                    .saturating_sub(SETTLE_CHUNKS)
                    .min(Self::needed_chunks())
            {
                self.last_hold = Some(UNSTEADY);
            }
        }

        if self.run.len() >= Self::needed_chunks() {
            let mean = self.run.iter().sum::<f64>() / self.run.len() as f64;
            self.result = Some(CalibrationProgress::Done {
                measured_dbfs: ms_to_db(mean, 0.0),
            });
        } else if self.elapsed_secs() >= TIMEOUT_SECS {
            let why = self.last_hold.unwrap_or(UNSTEADY);
            self.result = Some(CalibrationProgress::Failed {
                reason: format!("Stopped after {TIMEOUT_SECS:.0} s: {why}"),
            });
        }
    }

    fn elapsed_secs(&self) -> f32 {
        (self.chunks as f64 * CHUNK_SECS) as f32
    }

    pub fn progress(&self) -> CalibrationProgress {
        if let Some(done) = &self.result {
            return done.clone();
        }
        CalibrationProgress::Listening {
            level_dbfs: self.last_level.max(-140.0),
            steady_secs: (self.run.len() as f64 * CHUNK_SECS) as f32,
            elapsed_secs: self.elapsed_secs(),
            hold: self.last_hold,
        }
    }

    pub fn is_finished(&self) -> bool {
        self.result.is_some()
    }
}

fn spread_db(run: &VecDeque<f64>) -> f32 {
    let (lo, hi) = run.iter().fold((f64::INFINITY, 0.0f64), |(lo, hi), &m| {
        (lo.min(m), hi.max(m))
    });
    if run.is_empty() {
        0.0
    } else {
        ms_to_db(hi, 0.0) - ms_to_db(lo, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    const RATE: u32 = 48_000;

    fn tone(amp: f64, secs: f64) -> Vec<f32> {
        (0..(RATE as f64 * secs) as usize)
            .map(|i| (amp * (2.0 * PI * 1_000.0 * i as f64 / RATE as f64).sin()) as f32)
            .collect()
    }

    /// Cheap pink-ish noise: white noise through a one-pole low-pass, seeded.
    fn noise(amp: f64, secs: f64) -> Vec<f32> {
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut lp = 0.0;
        (0..(RATE as f64 * secs) as usize)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let white = (state >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0;
                lp += (white - lp) * 0.3;
                (amp * lp) as f32
            })
            .collect()
    }

    #[test]
    fn a_94_db_calibrator_tone_gives_its_level() {
        let mut cal = Calibrator::new(Weighting::C, RATE);
        cal.feed(&tone(0.1, 10.0));
        match cal.progress() {
            CalibrationProgress::Done { measured_dbfs } => {
                assert!((measured_dbfs - -23.01).abs() < 0.05, "{measured_dbfs}")
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn steady_noise_finishes_after_the_steady_stretch() {
        let mut cal = Calibrator::new(Weighting::A, RATE);
        cal.feed(&noise(0.2, 3.0));
        assert!(matches!(
            cal.progress(),
            CalibrationProgress::Listening { hold: None, .. }
        ));
        cal.feed(&noise(0.2, 5.0));
        assert!(cal.is_finished(), "{:?}", cal.progress());
    }

    #[test]
    fn a_level_jump_restarts_the_steady_count() {
        let mut cal = Calibrator::new(Weighting::C, RATE);
        cal.feed(&tone(0.1, 4.0));
        cal.feed(&tone(0.4, 1.0)); // someone turned it up
        let CalibrationProgress::Listening { steady_secs, .. } = cal.progress() else {
            panic!("finished early: {:?}", cal.progress());
        };
        assert!(steady_secs <= 1.0, "{steady_secs}");
        cal.feed(&tone(0.4, 5.0));
        match cal.progress() {
            CalibrationProgress::Done { measured_dbfs } => {
                assert!((measured_dbfs - -10.97).abs() < 0.1, "{measured_dbfs}")
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn refuses_clipping_and_silence() {
        let mut cal = Calibrator::new(Weighting::C, RATE);
        cal.feed(&tone(1.0, 2.0));
        assert!(matches!(
            cal.progress(),
            CalibrationProgress::Listening {
                hold: Some(CLIPPING),
                ..
            }
        ));

        let mut quiet = Calibrator::new(Weighting::A, RATE);
        quiet.feed(&tone(0.0001, TIMEOUT_SECS as f64 + 1.0));
        match quiet.progress() {
            CalibrationProgress::Failed { reason } => {
                assert!(reason.contains(TOO_QUIET), "{reason}")
            }
            other => panic!("{other:?}"),
        }
    }
}
