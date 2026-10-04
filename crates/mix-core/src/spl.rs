//! Room loudness (SPL) readings, A and C weighted.
//!
//! The audio engine measures one input (ideally a measurement mic in the room)
//! and reports these. Anything that wants room loudness, such as auto-mix
//! loudness targets, reads [`SplReading`] and never touches the audio itself.
//!
//! Every level here is in dB SPL *as best we know it*: the measured dBFS plus
//! the calibration offset. Until someone calibrates against a reference
//! meter, [`SplReading::calibrated`] is false and the numbers are estimates
//! that are only good for comparing one moment to another.

use serde::{Deserialize, Serialize};

/// Frequency weighting. A follows how loud quiet sound seems to the ear (the
/// usual "dBA" limit); C keeps the low end, so it shows how much the bass and
/// kick are adding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Weighting {
    A,
    C,
}

/// Which input to measure and how to turn its dBFS into dB SPL.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplConfig {
    /// 0-based input on the audio device; `None` turns the meter off.
    pub source: Option<u16>,
    /// dB SPL = dBFS (RMS) + this.
    pub offset_db: f32,
    /// True once the offset came from a reference meter or calibrator.
    pub calibrated: bool,
}

impl SplConfig {
    /// A rough guess for a measurement mic at typical preamp gain. Wrong by
    /// however much the gain differs, which is why readings stay labelled
    /// uncalibrated until someone checks them.
    pub const DEFAULT_OFFSET_DB: f32 = 120.0;
    pub const OFFSET_RANGE: std::ops::RangeInclusive<f32> = 60.0..=180.0;

    /// Keeps the offset inside a range a real mic and preamp could produce.
    pub fn sanitized(mut self) -> Self {
        if !self.offset_db.is_finite() {
            self.offset_db = Self::DEFAULT_OFFSET_DB;
        }
        self.offset_db = self
            .offset_db
            .clamp(*Self::OFFSET_RANGE.start(), *Self::OFFSET_RANGE.end());
        self
    }
}

impl Default for SplConfig {
    fn default() -> Self {
        Self {
            source: None,
            offset_db: Self::DEFAULT_OFFSET_DB,
            calibrated: false,
        }
    }
}

/// One A/C pair of levels in dB SPL.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcLevel {
    pub a: f32,
    pub c: f32,
}

/// The room's loudness right now and over the last stretches of time.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplReading {
    /// The input being measured.
    pub source: u16,
    pub calibrated: bool,
    /// Fast time weighting (125 ms): what a handheld meter shows moment to moment.
    pub fast: AcLevel,
    /// Slow time weighting (1 s): steadier, easier to read during a song.
    pub slow: AcLevel,
    /// Equivalent continuous level (Leq) over the last minute.
    pub leq_1m: AcLevel,
    /// Leq over the last 15 minutes, the usual window for church loudness guidelines.
    pub leq_15m: AcLevel,
    /// Leq since the meter was last reset (usually the whole service).
    pub leq_total: AcLevel,
    /// Loudest fast A level since the reset (LAFmax).
    pub a_max: f32,
    /// Highest instantaneous C-weighted peak since the reset (LCpeak).
    pub c_peak: f32,
    /// Seconds measured since the reset.
    pub seconds: f64,
}

/// One second of history: the Leq of that second, A and C.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplPoint {
    /// Seconds since the reset, at the end of this second.
    pub t: f64,
    pub a: f32,
    pub c: f32,
}
