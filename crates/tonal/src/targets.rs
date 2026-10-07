//! What "good" sounds like for each kind of source, and the high-pass rules.
//!
//! A target is a smooth curve: roughly how a well-EQ'd source of that role
//! spreads its energy, relative to its own average. They are authored from
//! common live-sound practice (like the auto-mix levels in the church
//! live-mix research) and are starting points to tune in beta, marked
//! [default] in docs/AIEQ.md. Learned church targets (from the EQ people
//! settle on) replace them per mic once a church has examples.

use automix::{ChannelRole, Nudges};

use crate::analyser::{band_hz, SPECTRUM_BANDS};

/// (frequency, dB) breakpoints, interpolated in log frequency.
type Curve = &'static [(f32, f32)];

const SPEECH: Curve = &[
    (60.0, -24.0),
    (100.0, -12.0),
    (150.0, -4.0),
    (250.0, 0.0),
    (500.0, 0.0),
    (1_000.0, -3.0),
    (2_000.0, -6.0),
    (3_000.0, -7.0),
    (5_000.0, -10.0),
    (8_000.0, -16.0),
    (12_000.0, -24.0),
    (20_000.0, -40.0),
];

const VOCAL: Curve = &[
    (60.0, -26.0),
    (100.0, -14.0),
    (150.0, -6.0),
    (250.0, -1.0),
    (500.0, 0.0),
    (1_000.0, -2.0),
    (2_000.0, -4.0),
    (3_000.0, -5.0),
    (5_000.0, -8.0),
    (8_000.0, -13.0),
    (12_000.0, -20.0),
    (20_000.0, -36.0),
];

const CHOIR: Curve = &[
    (60.0, -28.0),
    (100.0, -16.0),
    (200.0, -4.0),
    (400.0, 0.0),
    (1_000.0, -2.0),
    (3_000.0, -6.0),
    (6_000.0, -10.0),
    (12_000.0, -20.0),
    (20_000.0, -36.0),
];

const ACOUSTIC: Curve = &[
    (50.0, -20.0),
    (80.0, -10.0),
    (120.0, -3.0),
    (250.0, 0.0),
    (500.0, -1.0),
    (1_000.0, -3.0),
    (2_000.0, -5.0),
    (5_000.0, -8.0),
    (10_000.0, -14.0),
    (20_000.0, -30.0),
];

const ELECTRIC: Curve = &[
    (60.0, -22.0),
    (80.0, -14.0),
    (150.0, -4.0),
    (300.0, 0.0),
    (1_000.0, 0.0),
    (2_000.0, -2.0),
    (4_000.0, -8.0),
    (8_000.0, -20.0),
    (20_000.0, -40.0),
];

const BASS: Curve = &[
    (30.0, -4.0),
    (40.0, 0.0),
    (80.0, 0.0),
    (150.0, -2.0),
    (300.0, -6.0),
    (800.0, -12.0),
    (2_000.0, -20.0),
    (5_000.0, -30.0),
    (20_000.0, -50.0),
];

const KICK: Curve = &[
    (30.0, -4.0),
    (50.0, 0.0),
    (70.0, 0.0),
    (100.0, -2.0),
    (300.0, -12.0),
    (1_000.0, -16.0),
    (3_000.0, -14.0),
    (6_000.0, -18.0),
    (10_000.0, -26.0),
    (20_000.0, -40.0),
];

const DRUMS: Curve = &[
    (60.0, -18.0),
    (100.0, -10.0),
    (200.0, -3.0),
    (500.0, -2.0),
    (1_000.0, -3.0),
    (3_000.0, -4.0),
    (6_000.0, -6.0),
    (10_000.0, -10.0),
    (20_000.0, -24.0),
];

const KEYS: Curve = &[
    (30.0, -14.0),
    (50.0, -6.0),
    (100.0, -2.0),
    (250.0, 0.0),
    (1_000.0, -3.0),
    (3_000.0, -7.0),
    (8_000.0, -14.0),
    (20_000.0, -30.0),
];

fn interpolate(curve: Curve, hz: f32) -> f32 {
    let x = hz.max(1.0).log2();
    let (first, last) = (curve[0], curve[curve.len() - 1]);
    if hz <= first.0 {
        return first.1;
    }
    if hz >= last.0 {
        return last.1;
    }
    for w in curve.windows(2) {
        let (a, b) = (w[0], w[1]);
        if hz <= b.0 {
            let t = (x - a.0.log2()) / (b.0.log2() - a.0.log2());
            return a.1 + t * (b.1 - a.1);
        }
    }
    last.1
}

/// The built-in curve for a role, or `None` for roles AI EQ leaves alone
/// (tracks, playback and anything unlabelled).
fn curve_for(role: ChannelRole) -> Option<Curve> {
    Some(match role {
        ChannelRole::Speech => SPEECH,
        ChannelRole::LeadVocal | ChannelRole::BackingVocal => VOCAL,
        ChannelRole::Choir => CHOIR,
        ChannelRole::AcousticGuitar => ACOUSTIC,
        ChannelRole::ElectricGuitar => ELECTRIC,
        ChannelRole::Bass => BASS,
        ChannelRole::Kick => KICK,
        ChannelRole::Drums => DRUMS,
        ChannelRole::KeysPads | ChannelRole::PianoOrgan => KEYS,
        ChannelRole::Playback | ChannelRole::Other => return None,
    })
}

/// The target for a role at each spectrum band, with the room nudges
/// applied: "Vocal presence" tilts the vocals' 2-6 kHz presence by up to
/// ±2 dB, "Low end" tilts kick and bass below 120 Hz by up to ±2 dB.
/// Soundcheck and suggestions only, never live.
pub fn target(role: ChannelRole, nudges: &Nudges) -> Option<Vec<f32>> {
    let curve = curve_for(role)?;
    let n = nudges.sanitized();
    let presence = n.vocal_presence_db * 2.0 / 3.0;
    let low = n.low_end_db * 2.0 / 3.0;
    Some(
        (0..SPECTRUM_BANDS)
            .map(|i| {
                let hz = band_hz(i);
                let mut db = interpolate(curve, hz);
                if role.is_vocal() && (2_000.0..=6_000.0).contains(&hz) {
                    db += presence;
                }
                if matches!(role, ChannelRole::Kick | ChannelRole::Bass) && hz <= 120.0 {
                    db += low;
                }
                db
            })
            .collect(),
    )
}

/// The high-pass AI EQ suggests for a role, and the most it may ever set.
/// `None`: never set (kick and bass only get one for rumble below 30 Hz,
/// organ, tracks and ambient never).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HpfRule {
    pub suggest_hz: f32,
    pub ceiling_hz: f32,
}

pub fn hpf_rule(role: ChannelRole) -> Option<HpfRule> {
    let r = |suggest_hz, ceiling_hz| HpfRule {
        suggest_hz,
        ceiling_hz,
    };
    Some(match role {
        ChannelRole::Speech => r(100.0, 150.0),
        ChannelRole::LeadVocal | ChannelRole::BackingVocal | ChannelRole::Choir => r(100.0, 120.0),
        ChannelRole::AcousticGuitar => r(80.0, 100.0),
        ChannelRole::ElectricGuitar => r(80.0, 90.0),
        ChannelRole::Drums => r(80.0, 100.0),
        ChannelRole::KeysPads => r(40.0, 60.0),
        ChannelRole::Kick | ChannelRole::Bass => r(25.0, 30.0),
        // Organ pedals, tracks and ambience keep their low end.
        ChannelRole::PianoOrgan | ChannelRole::Playback | ChannelRole::Other => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyser::band_of;

    #[test]
    fn targets_follow_their_breakpoints() {
        let t = target(ChannelRole::Speech, &Nudges::default()).unwrap();
        assert!((t[band_of(250.0).unwrap()]).abs() < 0.5);
        assert!(t[band_of(8_000.0).unwrap()] < -14.0);
        assert!(target(ChannelRole::Playback, &Nudges::default()).is_none());
    }

    #[test]
    fn presence_nudge_lifts_vocals_only() {
        let up = Nudges {
            vocal_presence_db: 3.0,
            ..Nudges::default()
        };
        let k = band_of(3_000.0).unwrap();
        let flat = target(ChannelRole::LeadVocal, &Nudges::default()).unwrap();
        let lifted = target(ChannelRole::LeadVocal, &up).unwrap();
        assert!((lifted[k] - flat[k] - 2.0).abs() < 1e-4);
        let kick = target(ChannelRole::Kick, &up).unwrap();
        assert_eq!(kick, target(ChannelRole::Kick, &Nudges::default()).unwrap());
    }

    #[test]
    fn hpf_ceilings_match_the_behaviour_rules() {
        assert_eq!(hpf_rule(ChannelRole::Speech).unwrap().ceiling_hz, 150.0);
        assert_eq!(hpf_rule(ChannelRole::LeadVocal).unwrap().ceiling_hz, 120.0);
        assert_eq!(
            hpf_rule(ChannelRole::ElectricGuitar).unwrap().ceiling_hz,
            90.0
        );
        assert_eq!(hpf_rule(ChannelRole::Bass).unwrap().ceiling_hz, 30.0);
        assert!(hpf_rule(ChannelRole::PianoOrgan).is_none());
        assert!(hpf_rule(ChannelRole::Playback).is_none());
    }
}
