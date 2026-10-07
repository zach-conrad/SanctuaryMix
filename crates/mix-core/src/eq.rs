//! Channel EQ: the input high-pass filter and four parametric bands.
//!
//! The layout and the value grids are the Allen & Heath dLive's (firmware 1.9
//! and 2.0 MIDI-over-TCP protocol): every value the app proposes is snapped to
//! a step the desk can actually hold, so what the UI shows is what the desk
//! will do. Bands are numbered 0 to 3 here; the UI shows them as 1 to 4.
//! Band 3 (the UI's "band 4") is reserved for SanctuaryMix's feedback notches.

use serde::{Deserialize, Serialize};

/// Number of parametric bands on a dLive input.
pub const BANDS: usize = 4;
/// The band AI EQ keeps free for feedback notches (the UI's band 4).
pub const NOTCH_BAND: u8 = 3;

pub const MIN_FREQ_HZ: f32 = 20.0;
pub const MAX_FREQ_HZ: f32 = 20_000.0;
pub const MIN_GAIN_DB: f32 = -15.0;
pub const MAX_GAIN_DB: f32 = 15.0;

/// The shape of one band. Bands 1 and 2 are always bells; band 0 can also be a
/// low shelf or a high-pass, band 3 a high shelf or a low-pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EqBandKind {
    Bell,
    LowShelf,
    HighShelf,
    LowPass,
    HighPass,
}

impl EqBandKind {
    /// Whether `band` (0-3) can take this shape on a dLive.
    pub fn allowed_on(self, band: u8) -> bool {
        match self {
            EqBandKind::Bell => band < BANDS as u8,
            EqBandKind::LowShelf | EqBandKind::HighPass => band == 0,
            EqBandKind::HighShelf | EqBandKind::LowPass => band == 3,
        }
    }
}

/// One parametric band.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EqBand {
    pub kind: EqBandKind,
    pub freq_hz: f32,
    /// Bandwidth in octaves, as the desk labels it (1.5 wide to 1/9 narrow).
    pub width: f32,
    pub gain_db: f32,
}

impl EqBand {
    pub const fn bell(freq_hz: f32, width: f32, gain_db: f32) -> Self {
        Self {
            kind: EqBandKind::Bell,
            freq_hz,
            width,
            gain_db,
        }
    }

    /// The same band snapped to values the desk can hold.
    pub fn snapped(self) -> Self {
        Self {
            kind: self.kind,
            freq_hz: grid::freq_from_value(grid::freq_value(self.freq_hz)),
            width: grid::width_from_value(grid::width_value(self.width)),
            gain_db: grid::gain_from_value(grid::gain_value(self.gain_db)),
        }
    }
}

/// The input high-pass filter (separate from the four bands).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hpf {
    pub on: bool,
    pub freq_hz: f32,
}

/// Everything on one input's EQ that SanctuaryMix can see and set.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelEq {
    pub hpf: Hpf,
    pub bands: [EqBand; BANDS],
}

impl Default for ChannelEq {
    /// A flat EQ, laid out like a fresh dLive input.
    fn default() -> Self {
        Self {
            hpf: Hpf {
                on: false,
                freq_hz: 80.0,
            },
            bands: [
                EqBand::bell(100.0, 1.0, 0.0),
                EqBand::bell(500.0, 1.0, 0.0),
                EqBand::bell(2_000.0, 1.0, 0.0),
                EqBand::bell(8_000.0, 1.0, 0.0),
            ],
        }
    }
}

impl ChannelEq {
    /// Applies one parameter change.
    pub fn apply(&mut self, change: &EqChange) {
        match *change {
            EqChange::BandKind { band, kind } => {
                if let Some(b) = self.bands.get_mut(band as usize) {
                    b.kind = kind;
                }
            }
            EqChange::BandFreq { band, hz } => {
                if let Some(b) = self.bands.get_mut(band as usize) {
                    b.freq_hz = hz;
                }
            }
            EqChange::BandWidth { band, width } => {
                if let Some(b) = self.bands.get_mut(band as usize) {
                    b.width = width;
                }
            }
            EqChange::BandGain { band, db } => {
                if let Some(b) = self.bands.get_mut(band as usize) {
                    b.gain_db = db;
                }
            }
            EqChange::HpfOn { on } => self.hpf.on = on,
            EqChange::HpfFreq { hz } => self.hpf.freq_hz = hz,
        }
    }

    /// The parameter changes that turn `self` into `to`, skipping what is
    /// already equal on the desk's grid. Band shape goes before frequency and
    /// gain so the desk never briefly applies a gain to the wrong shape.
    pub fn diff(&self, to: &ChannelEq) -> Vec<EqChange> {
        let mut out = Vec::new();
        if grid::hpf_value(self.hpf.freq_hz) != grid::hpf_value(to.hpf.freq_hz) {
            out.push(EqChange::HpfFreq { hz: to.hpf.freq_hz });
        }
        if self.hpf.on != to.hpf.on {
            out.push(EqChange::HpfOn { on: to.hpf.on });
        }
        for (i, (a, b)) in self.bands.iter().zip(to.bands.iter()).enumerate() {
            let band = i as u8;
            if a.kind != b.kind {
                out.push(EqChange::BandKind { band, kind: b.kind });
            }
            if grid::freq_value(a.freq_hz) != grid::freq_value(b.freq_hz) {
                out.push(EqChange::BandFreq {
                    band,
                    hz: b.freq_hz,
                });
            }
            if grid::width_value(a.width) != grid::width_value(b.width) {
                out.push(EqChange::BandWidth {
                    band,
                    width: b.width,
                });
            }
            if grid::gain_value(a.gain_db) != grid::gain_value(b.gain_db) {
                out.push(EqChange::BandGain {
                    band,
                    db: b.gain_db,
                });
            }
        }
        out
    }

    /// Every value snapped to the desk's grid.
    pub fn snapped(mut self) -> Self {
        self.hpf.freq_hz = grid::hpf_from_value(grid::hpf_value(self.hpf.freq_hz));
        for b in &mut self.bands {
            *b = b.snapped();
        }
        self
    }
}

/// One EQ parameter changing on one channel, as the desk sends and takes them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "param", rename_all = "camelCase")]
pub enum EqChange {
    BandKind { band: u8, kind: EqBandKind },
    BandFreq { band: u8, hz: f32 },
    BandWidth { band: u8, width: f32 },
    BandGain { band: u8, db: f32 },
    HpfOn { on: bool },
    HpfFreq { hz: f32 },
}

impl EqChange {
    /// The band this touches, or `None` for the high-pass filter.
    pub fn band(&self) -> Option<u8> {
        match *self {
            EqChange::BandKind { band, .. }
            | EqChange::BandFreq { band, .. }
            | EqChange::BandWidth { band, .. }
            | EqChange::BandGain { band, .. } => Some(band),
            EqChange::HpfOn { .. } | EqChange::HpfFreq { .. } => None,
        }
    }

    /// Same parameter, same value on the desk's grid.
    pub fn same_value(&self, other: &EqChange) -> bool {
        use EqChange::*;
        match (*self, *other) {
            (BandKind { band: a, kind: x }, BandKind { band: b, kind: y }) => a == b && x == y,
            (BandFreq { band: a, hz: x }, BandFreq { band: b, hz: y }) => {
                a == b && grid::freq_value(x) == grid::freq_value(y)
            }
            (BandWidth { band: a, width: x }, BandWidth { band: b, width: y }) => {
                a == b && grid::width_value(x) == grid::width_value(y)
            }
            (BandGain { band: a, db: x }, BandGain { band: b, db: y }) => {
                a == b && grid::gain_value(x) == grid::gain_value(y)
            }
            (HpfOn { on: x }, HpfOn { on: y }) => x == y,
            (HpfFreq { hz: x }, HpfFreq { hz: y }) => grid::hpf_value(x) == grid::hpf_value(y),
            _ => false,
        }
    }

    /// Same parameter (whatever the value).
    pub fn same_param(&self, other: &EqChange) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other) && self.band() == other.band()
    }
}

/// The dLive's 7-bit value grids (A&H dLive MIDI over TCP, V1.9/V2.0).
pub mod grid {
    use super::*;

    /// Band widths in octaves, indexed by the desk's value (0 = 1.5, 24 = 1/9).
    pub const WIDTHS: [f32; 25] = [
        1.5,
        1.4,
        1.3,
        1.2,
        1.1,
        1.0,
        0.95,
        0.9,
        0.85,
        0.8,
        0.75,
        0.7,
        2.0 / 3.0,
        0.6,
        0.55,
        0.5,
        0.45,
        0.4,
        1.0 / 3.0,
        0.3,
        0.25,
        0.2,
        1.0 / 6.0,
        0.13,
        1.0 / 9.0,
    ];
    /// Widest and narrowest the desk offers.
    pub const WIDEST: f32 = WIDTHS[0];
    pub const NARROWEST: f32 = WIDTHS[24];

    const FREQ_DIVISOR: f32 = 45_922.0;
    const HPF_DIVISOR: f32 = 41_314.0;

    fn log_value(hz: f32, divisor: f32) -> u8 {
        let hz = if hz.is_finite() { hz } else { 1_000.0 };
        let hz = hz.clamp(MIN_FREQ_HZ, MAX_FREQ_HZ);
        let v = 127.0 * (4608.0 * (hz / 4.0).log2() - 10_699.0) / divisor;
        // The spec truncates (INT); a hair of slack keeps exact grid values stable.
        (v + 1e-3).floor().clamp(0.0, 127.0) as u8
    }

    fn log_hz(value: u8, divisor: f32) -> f32 {
        let v = value.min(127) as f32;
        // Middle of the step, so round trips land back on the same value.
        let x = ((v + 0.5) * divisor / 127.0 + 10_699.0) / 4608.0;
        (4.0 * x.exp2()).clamp(MIN_FREQ_HZ, MAX_FREQ_HZ)
    }

    pub fn freq_value(hz: f32) -> u8 {
        log_value(hz, FREQ_DIVISOR)
    }

    pub fn freq_from_value(value: u8) -> f32 {
        if value == 0 {
            return MIN_FREQ_HZ;
        }
        log_hz(value, FREQ_DIVISOR)
    }

    pub fn hpf_value(hz: f32) -> u8 {
        log_value(hz, HPF_DIVISOR)
    }

    pub fn hpf_from_value(value: u8) -> f32 {
        if value == 0 {
            return MIN_FREQ_HZ;
        }
        log_hz(value, HPF_DIVISOR)
    }

    /// `(gain + 15) * 126 / 30`, rounded: about 0.24 dB per step.
    pub fn gain_value(db: f32) -> u8 {
        let db = if db.is_finite() { db } else { 0.0 };
        ((db.clamp(MIN_GAIN_DB, MAX_GAIN_DB) + 15.0) * 126.0 / 30.0)
            .round()
            .clamp(0.0, 127.0) as u8
    }

    /// The desk gain nearest `db` that stays inside `lo..=hi`.
    pub fn gain_within(db: f32, lo: f32, hi: f32) -> f32 {
        let mut v = gain_value(db.clamp(lo, hi));
        while v > 0 && gain_from_value(v) > hi + 1e-4 {
            v -= 1;
        }
        while v < 126 && gain_from_value(v) < lo - 1e-4 {
            v += 1;
        }
        gain_from_value(v)
    }

    pub fn gain_from_value(value: u8) -> f32 {
        (value.min(126) as f32 * 30.0 / 126.0 - 15.0).clamp(MIN_GAIN_DB, MAX_GAIN_DB)
    }

    /// The nearest width the desk has (compared in log terms).
    pub fn width_value(width: f32) -> u8 {
        let w = if width.is_finite() && width > 0.0 {
            width
        } else {
            1.0
        };
        WIDTHS
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| (a.ln() - w.ln()).abs().total_cmp(&(b.ln() - w.ln()).abs()))
            .map(|(i, _)| i as u8)
            .unwrap_or(5)
    }

    pub fn width_from_value(value: u8) -> f32 {
        WIDTHS[(value as usize).min(WIDTHS.len() - 1)]
    }

    pub fn kind_value(kind: EqBandKind) -> u8 {
        match kind {
            EqBandKind::Bell => 0,
            EqBandKind::LowShelf => 1,
            EqBandKind::HighShelf => 2,
            EqBandKind::LowPass => 3,
            EqBandKind::HighPass => 4,
        }
    }

    pub fn kind_from_value(value: u8) -> Option<EqBandKind> {
        Some(match value {
            0 => EqBandKind::Bell,
            1 => EqBandKind::LowShelf,
            2 => EqBandKind::HighShelf,
            3 => EqBandKind::LowPass,
            4 => EqBandKind::HighPass,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequency_grid_matches_the_spec() {
        assert_eq!(grid::freq_value(20.0), 0);
        assert_eq!(grid::freq_value(20_000.0), 127);
        assert_eq!(grid::freq_value(1_000.0), 0x47);
        for v in 1..=127u8 {
            assert_eq!(grid::freq_value(grid::freq_from_value(v)), v, "value {v}");
        }
    }

    #[test]
    fn hpf_grid_round_trips() {
        assert_eq!(grid::hpf_value(20.0), 0);
        for v in 1..=127u8 {
            let hz = grid::hpf_from_value(v);
            if hz < MAX_FREQ_HZ {
                assert_eq!(grid::hpf_value(hz), v, "value {v}");
            }
        }
    }

    #[test]
    fn gain_grid_matches_the_spec() {
        assert_eq!(grid::gain_value(-15.0), 0);
        assert_eq!(grid::gain_value(0.0), 0x3F);
        assert_eq!(grid::gain_value(15.0), 126);
        assert!((grid::gain_from_value(0x3F)).abs() < 1e-6);
        for v in 0..=126u8 {
            assert_eq!(grid::gain_value(grid::gain_from_value(v)), v);
        }
        let g = grid::gain_within(3.0, -6.0, 3.0);
        assert!(g <= 3.0 && g > 2.7, "{g}");
        let g = grid::gain_within(-9.0, -6.0, 3.0);
        assert!((-6.0..-5.7).contains(&g), "{g}");
    }

    #[test]
    fn widths_snap_to_the_nearest_step() {
        assert_eq!(grid::width_value(1.5), 0);
        assert_eq!(grid::width_value(5.0), 0);
        assert_eq!(grid::width_value(0.11), 24);
        assert_eq!(grid::width_value(1.0 / 3.0), 18);
        assert_eq!(grid::width_value(0.01), 24);
    }

    #[test]
    fn band_kinds_follow_the_desk() {
        assert!(EqBandKind::HighPass.allowed_on(0));
        assert!(!EqBandKind::HighPass.allowed_on(3));
        assert!(EqBandKind::HighShelf.allowed_on(3));
        assert!(EqBandKind::Bell.allowed_on(2));
        assert!(!EqBandKind::LowShelf.allowed_on(1));
    }

    #[test]
    fn diff_lists_only_what_differs_on_the_grid() {
        let a = ChannelEq::default();
        let mut b = a;
        b.bands[1].gain_db = -3.0;
        b.bands[1].freq_hz = 320.0;
        b.hpf.on = true;
        b.bands[2].gain_db = 0.05; // rounds to the same step as 0.0
        let d = a.diff(&b);
        assert_eq!(d.len(), 3, "{d:?}");
        let mut c = a;
        for ch in &d {
            c.apply(ch);
        }
        assert!(c.diff(&b).is_empty());
    }

    #[test]
    fn changes_serialize_with_a_param_tag() {
        let c = EqChange::BandGain { band: 3, db: -6.0 };
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(json, r#"{"param":"bandGain","band":3,"db":-6.0}"#);
    }
}
