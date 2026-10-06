//! The dLive EQ as maths: magnitude response of the input high-pass filter
//! and the four bands (RBJ "Audio EQ Cookbook" biquads at 48 kHz).
//!
//! The UI draws the same curves (`src/lib/eqCurve.ts`), so the curve on
//! screen is the one used for every decision. Band width is taken as
//! bandwidth in octaves, the desk's own label. A&H doesn't publish the
//! exact filter shapes; a measurement session on a real desk (pink noise
//! through each width and type) should confirm or replace this table.

use std::f32::consts::PI;

use mix_core::eq::{ChannelEq, EqBand, EqBandKind, Hpf};

pub const SAMPLE_RATE: f32 = 48_000.0;
/// Shelves and pass bands use a Butterworth Q.
const SHELF_Q: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// Q for a bell `width` octaves wide.
pub fn width_to_q(width: f32) -> f32 {
    let n = width.clamp(0.05, 4.0);
    let p = n.exp2();
    p.sqrt() / (p - 1.0)
}

#[derive(Debug, Clone, Copy)]
struct Biquad([f32; 5]);

impl Biquad {
    fn new(kind: EqBandKind, freq_hz: f32, q: f32, gain_db: f32) -> Self {
        let w0 = 2.0 * PI * freq_hz.clamp(10.0, SAMPLE_RATE * 0.45) / SAMPLE_RATE;
        let (sin, cos) = w0.sin_cos();
        let a = 10f32.powf(gain_db / 40.0);
        let alpha = sin / (2.0 * q);
        let (b0, b1, b2, a0, a1, a2) = match kind {
            EqBandKind::Bell => (
                1.0 + alpha * a,
                -2.0 * cos,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * cos,
                1.0 - alpha / a,
            ),
            EqBandKind::HighPass => (
                (1.0 + cos) / 2.0,
                -(1.0 + cos),
                (1.0 + cos) / 2.0,
                1.0 + alpha,
                -2.0 * cos,
                1.0 - alpha,
            ),
            EqBandKind::LowPass => (
                (1.0 - cos) / 2.0,
                1.0 - cos,
                (1.0 - cos) / 2.0,
                1.0 + alpha,
                -2.0 * cos,
                1.0 - alpha,
            ),
            EqBandKind::LowShelf => {
                let s = 2.0 * a.sqrt() * alpha;
                (
                    a * (a + 1.0 - (a - 1.0) * cos + s),
                    2.0 * a * (a - 1.0 - (a + 1.0) * cos),
                    a * (a + 1.0 - (a - 1.0) * cos - s),
                    a + 1.0 + (a - 1.0) * cos + s,
                    -2.0 * (a - 1.0 + (a + 1.0) * cos),
                    a + 1.0 + (a - 1.0) * cos - s,
                )
            }
            EqBandKind::HighShelf => {
                let s = 2.0 * a.sqrt() * alpha;
                (
                    a * (a + 1.0 + (a - 1.0) * cos + s),
                    -2.0 * a * (a - 1.0 + (a + 1.0) * cos),
                    a * (a + 1.0 + (a - 1.0) * cos - s),
                    a + 1.0 - (a - 1.0) * cos + s,
                    2.0 * (a - 1.0 - (a + 1.0) * cos),
                    a + 1.0 - (a - 1.0) * cos - s,
                )
            }
        };
        Self([b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0])
    }

    fn magnitude_db(&self, freq_hz: f32) -> f32 {
        let [b0, b1, b2, a1, a2] = self.0;
        let w = 2.0 * PI * freq_hz / SAMPLE_RATE;
        let (s1, c1) = w.sin_cos();
        let (s2, c2) = (2.0 * w).sin_cos();
        let nr = b0 + b1 * c1 + b2 * c2;
        let ni = -(b1 * s1 + b2 * s2);
        let dr = 1.0 + a1 * c1 + a2 * c2;
        let di = -(a1 * s1 + a2 * s2);
        10.0 * ((nr * nr + ni * ni).max(1e-20) / (dr * dr + di * di).max(1e-20)).log10()
    }
}

fn band_filter(b: &EqBand) -> Biquad {
    let q = match b.kind {
        EqBandKind::Bell => width_to_q(b.width),
        _ => SHELF_Q,
    };
    Biquad::new(b.kind, b.freq_hz, q, b.gain_db)
}

/// One band's contribution at `freq_hz`, in dB.
pub fn band_response(b: &EqBand, freq_hz: f32) -> f32 {
    if b.kind == EqBandKind::Bell && b.gain_db.abs() < 1e-3 {
        return 0.0;
    }
    band_filter(b).magnitude_db(freq_hz)
}

/// The high-pass filter's contribution (12 dB per octave), in dB.
pub fn hpf_response(hpf: &Hpf, freq_hz: f32) -> f32 {
    if !hpf.on {
        return 0.0;
    }
    Biquad::new(EqBandKind::HighPass, hpf.freq_hz, SHELF_Q, 0.0).magnitude_db(freq_hz)
}

/// The whole channel EQ at `freq_hz`, in dB.
pub fn response(eq: &ChannelEq, freq_hz: f32) -> f32 {
    hpf_response(&eq.hpf, freq_hz)
        + eq.bands
            .iter()
            .map(|b| band_response(b, freq_hz))
            .sum::<f32>()
}

/// [`response`] at each of `freqs`.
pub fn curve(eq: &ChannelEq, freqs: &[f32]) -> Vec<f32> {
    freqs.iter().map(|&f| response(eq, f)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bell_peaks_at_its_gain() {
        let b = EqBand::bell(1_000.0, 1.0, -6.0);
        assert!((band_response(&b, 1_000.0) + 6.0).abs() < 0.05);
        assert!(band_response(&b, 100.0).abs() < 0.3);
        assert!(band_response(&b, 10_000.0).abs() < 0.3);
    }

    #[test]
    fn narrower_bells_touch_less() {
        let wide = EqBand::bell(2_500.0, 1.5, -6.0);
        let narrow = EqBand::bell(2_500.0, 1.0 / 9.0, -6.0);
        assert!(band_response(&narrow, 3_000.0) > band_response(&wide, 3_000.0) + 3.0);
    }

    #[test]
    fn one_octave_is_q_1_4() {
        assert!((width_to_q(1.0) - 1.414).abs() < 0.01);
    }

    #[test]
    fn hpf_is_3_db_down_at_its_corner_and_12_db_an_octave_below() {
        let hpf = Hpf {
            on: true,
            freq_hz: 100.0,
        };
        assert!((hpf_response(&hpf, 100.0) + 3.0).abs() < 0.2);
        assert!((hpf_response(&hpf, 25.0) + 24.0).abs() < 1.5);
        assert!(hpf_response(&hpf, 1_000.0).abs() < 0.1);
        assert_eq!(
            hpf_response(
                &Hpf {
                    on: false,
                    freq_hz: 100.0
                },
                30.0
            ),
            0.0
        );
    }

    #[test]
    fn shelves_reach_their_gain() {
        let mut b = EqBand::bell(200.0, 1.0, 4.0);
        b.kind = EqBandKind::LowShelf;
        assert!((band_response(&b, 30.0) - 4.0).abs() < 0.2);
        assert!(band_response(&b, 5_000.0).abs() < 0.2);
    }
}
