//! Turning what a channel sounds like into a soundcheck EQ proposal.
//!
//! Given the measured long-term spectrum, the role's target and the desk's
//! current EQ, it finds the smallest EQ that moves the channel toward its
//! target, on values the desk can actually hold:
//!
//! 1. A high-pass at the role's suggestion (never above its ceiling), when the
//!    role has one and the desk's own high-pass is off.
//! 2. Up to three bells on bands 1 to 3 (0-2 here), placed greedily on the
//!    largest weighted error, cut-first: boosts only for clarity on voices
//!    (2.5 to 6 kHz), never near a known feedback frequency. Each band then
//!    tries the desk's wide widths and nearby frequency steps and keeps the
//!    one that fits best.
//! 3. Band 4 (3 here) is never touched: it belongs to feedback notches.
//!
//! The error is weighted toward 200 Hz to 5 kHz, where mud, honk and
//! harshness live. A proposal that doesn't improve things enough is dropped
//! ("Sounds good"). [`crate::guardrails`] checks the result again.

use automix::{ChannelRole, Nudges};
use mix_core::eq::{grid, ChannelEq, EqBand};

use crate::analyser::{band_hz, SPECTRUM_BANDS};
use crate::guardrails::{self, hard};
use crate::model;
use crate::targets::{self, HpfRule};
use crate::text;
use crate::types::EqProposal;

/// Below this weighted error (dB) a spot is left alone.
const MIN_ERROR_DB: f32 = 1.5;
/// Corrections smaller than this aren't worth a band.
const MIN_GAIN_DB: f32 = 1.0;
/// A proposal must improve the weighted error by this much (dB RMS)...
const MIN_IMPROVEMENT_DB: f32 = 0.5;
/// How much of the measured error a correction takes out (leave some character).
const CORRECTION_SHARE: f32 = 0.8;
/// Low-end excess (dB over target) that earns a high-pass on non-voice roles.
const RUMBLE_DB: f32 = 3.0;

/// Everything the solver needs about one channel.
pub struct SolveInput<'a> {
    pub role: ChannelRole,
    pub nudges: &'a Nudges,
    /// What the mic hears, per spectrum band, before the desk's EQ.
    pub measured_db: &'a [f32],
    /// What is on the desk now.
    pub desk: ChannelEq,
    /// Frequencies that have rung on this mic; no boosts near them.
    pub feedback_hz: &'a [f32],
}

fn weight(hz: f32) -> f32 {
    match hz {
        f if !(40.0..=16_000.0).contains(&f) => 0.0,
        f if !(60.0..=12_000.0).contains(&f) => 0.3,
        f if !(100.0..=10_000.0).contains(&f) => 0.5,
        f if !(200.0..=5_000.0).contains(&f) => 0.7,
        _ => 1.0,
    }
}

fn freqs() -> Vec<f32> {
    (0..SPECTRUM_BANDS).map(band_hz).collect()
}

/// Error after EQ `eq`, level-aligned and smoothed over about 1/3 octave.
fn error(measured: &[f32], target: &[f32], eq: &ChannelEq, freqs: &[f32]) -> Vec<f32> {
    let post: Vec<f32> = measured
        .iter()
        .zip(freqs)
        .map(|(m, &f)| m + model::response(eq, f))
        .collect();
    let raw: Vec<f32> = post.iter().zip(target).map(|(p, t)| p - t).collect();
    // Level-align on the weighted mean, so only the shape counts.
    let (mut num, mut den) = (0.0, 0.0);
    for (e, &f) in raw.iter().zip(freqs) {
        let w = weight(f);
        num += w * e;
        den += w;
    }
    let offset = if den > 0.0 { num / den } else { 0.0 };
    (0..raw.len())
        .map(|i| {
            let lo = i.saturating_sub(1);
            let hi = (i + 1).min(raw.len() - 1);
            raw[lo..=hi].iter().sum::<f32>() / (hi - lo + 1) as f32 - offset
        })
        .collect()
}

fn weighted_rms(err: &[f32], freqs: &[f32]) -> f32 {
    let (mut num, mut den) = (0.0, 0.0);
    for (e, &f) in err.iter().zip(freqs) {
        let w = weight(f);
        num += w * e * e;
        den += w;
    }
    if den > 0.0 {
        (num / den).sqrt()
    } else {
        0.0
    }
}

/// What a correction at `hz` is about, for the title and reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Spot {
    Rumble,
    Boom,
    Mud,
    Honk,
    Harsh,
    Hiss,
    Dull,
    Thin,
}

impl Spot {
    fn of(hz: f32, cut: bool) -> Self {
        match (hz, cut) {
            (f, true) if f < 200.0 => Spot::Boom,
            (f, true) if f < 700.0 => Spot::Mud,
            (f, true) if f < 2_000.0 => Spot::Honk,
            (f, true) if f < 5_000.0 => Spot::Harsh,
            (_, true) => Spot::Hiss,
            (f, false) if f < 1_000.0 => Spot::Thin,
            _ => Spot::Dull,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Spot::Rumble => "less rumble",
            Spot::Boom => "less boomy",
            Spot::Mud => "less boxy",
            Spot::Honk => "less nasal",
            Spot::Harsh => "less harsh on loud notes",
            Spot::Hiss => "softer S sounds",
            Spot::Dull => "clearer",
            Spot::Thin => "fuller",
        }
    }

    fn heard(self, at: &str) -> String {
        match self {
            Spot::Rumble => "some stage rumble".to_string(),
            Spot::Boom => format!("a boomy buildup around {at}"),
            Spot::Mud => format!("a boxy buildup around {at}"),
            Spot::Honk => format!("a nasal honk near {at}"),
            Spot::Harsh => format!("an edge near {at}"),
            Spot::Hiss => format!("sharp S sounds near {at}"),
            Spot::Dull => format!("not much clarity around {at}"),
            Spot::Thin => format!("a thin spot around {at}"),
        }
    }
}

fn sentence(spots: &[(Spot, f32)]) -> String {
    let parts: Vec<String> = spots
        .iter()
        .map(|&(s, hz)| s.heard(&text::hz(hz)))
        .collect();
    let list = match parts.len() {
        0 => return "It sounds close to how this kind of source should.".into(),
        1 => parts[0].clone(),
        2 => format!("{} and {}", parts[0], parts[1]),
        _ => format!(
            "{}, and {}",
            parts[..parts.len() - 1].join(", "),
            parts[parts.len() - 1]
        ),
    };
    format!("There's {list}.")
}

fn title(spots: &[(Spot, f32)]) -> String {
    let mut words: Vec<&str> = Vec::new();
    for &(s, _) in spots {
        let w = s.title();
        if !words.contains(&w) {
            words.push(w);
        }
    }
    words.truncate(2);
    let joined = words.join(", ");
    let mut c = joined.chars();
    match c.next() {
        Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn near_feedback(hz: f32, feedback: &[f32]) -> bool {
    feedback
        .iter()
        .any(|&f| (hz / f).log2().abs() <= guardrails::FEEDBACK_GUARD_OCTAVES)
}

/// The high-pass for this proposal, and whether it is a change worth naming.
fn high_pass(input: &SolveInput, rule: Option<HpfRule>, target: &[f32]) -> (bool, f32, bool) {
    let desk = input.desk.hpf;
    // A high-pass a person turned on stays exactly as they set it.
    if desk.on {
        return (true, desk.freq_hz, false);
    }
    let Some(rule) = rule else {
        return (false, desk.freq_hz, false);
    };
    let hz = rule.suggest_hz.min(rule.ceiling_hz);
    let voice = input.role.is_vocal();
    // Instruments only get one when there's real energy under the cutoff.
    let rumble = {
        let f = freqs();
        let below: Vec<usize> = (0..SPECTRUM_BANDS).filter(|&i| f[i] < hz).collect();
        let err = error(input.measured_db, target, &ChannelEq::default(), &f);
        !below.is_empty()
            && below.iter().map(|&i| err[i]).sum::<f32>() / below.len() as f32 > RUMBLE_DB
    };
    if voice || rumble {
        // Snap down so the grid never lands above the ceiling.
        let mut v = grid::hpf_value(hz);
        while v > 0 && grid::hpf_from_value(v) > rule.ceiling_hz {
            v -= 1;
        }
        (true, grid::hpf_from_value(v), true)
    } else {
        (false, desk.freq_hz, false)
    }
}

/// A soundcheck proposal, or `None` when the channel already sounds right
/// (or its role is one AI EQ leaves alone).
pub fn solve(input: &SolveInput) -> Option<EqProposal> {
    let target = targets::target(input.role, input.nudges)?;
    if input.measured_db.len() != SPECTRUM_BANDS {
        return None;
    }
    let f = freqs();
    let rule = targets::hpf_rule(input.role);
    let mut eq = input.desk;
    let (hpf_on, hpf_hz, hpf_changed) = high_pass(input, rule, &target);
    eq.hpf.on = hpf_on;
    eq.hpf.freq_hz = hpf_hz;
    // Bands 1-3 start flat; band 4 stays as the desk has it.
    let defaults = ChannelEq::default().bands;
    eq.bands[..3].copy_from_slice(&defaults[..3]);

    let start_err = weighted_rms(&error(input.measured_db, &target, &input.desk, &f), &f);
    let mut placed: Vec<(EqBand, Spot)> = Vec::new();
    let mut spots: Vec<(Spot, f32)> = Vec::new();
    if hpf_changed {
        spots.push((Spot::Rumble, hpf_hz));
    }

    for slot in 0..3 {
        let err = error(input.measured_db, &target, &eq, &f);
        // Best spot to work on: cuts where it's too much, boosts only for clarity.
        let mut best: Option<(usize, f32, bool)> = None;
        for i in 0..SPECTRUM_BANDS {
            let hz = f[i];
            if !(60.0..=12_000.0).contains(&hz) {
                continue;
            }
            if placed
                .iter()
                .any(|(b, _)| (hz / b.freq_hz).log2().abs() < 0.5)
            {
                continue;
            }
            let w = weight(hz);
            let (score, cut) = if err[i] > 0.0 {
                (w * err[i], true)
            } else if input.role.is_vocal()
                && (2_500.0..=6_000.0).contains(&hz)
                && !near_feedback(hz, input.feedback_hz)
            {
                // Boosting is riskier than cutting (feedback margin).
                (w * -err[i] * 0.6, false)
            } else {
                continue;
            };
            if best.is_none_or(|(_, s, _)| score > s) {
                best = Some((i, score, cut));
            }
        }
        let Some((i, score, cut)) = best else { break };
        if score < MIN_ERROR_DB {
            break;
        }
        let wanted = (-err[i] * CORRECTION_SHARE).clamp(hard::TONE_MAX_CUT_DB, hard::MAX_BOOST_DB);
        if wanted.abs() < MIN_GAIN_DB {
            break;
        }
        // Try the desk's wide widths and nearby frequencies; keep the best fit.
        let base_v = grid::freq_value(f[i]);
        let mut choice: Option<(EqBand, f32)> = None;
        for dv in -2i16..=2 {
            let v = (base_v as i16 + dv).clamp(0, 127) as u8;
            for wv in 0..=hard::TONE_NARROWEST_WIDTH_VALUE {
                let hz = grid::freq_from_value(v);
                if wanted > 0.0 && near_feedback(hz, input.feedback_hz) {
                    continue;
                }
                let mut band = EqBand::bell(hz, grid::width_from_value(wv), wanted).snapped();
                band.gain_db = grid::gain_within(wanted, hard::TONE_MAX_CUT_DB, hard::MAX_BOOST_DB);
                let mut trial = eq;
                trial.bands[slot] = band;
                let e = weighted_rms(&error(input.measured_db, &target, &trial, &f), &f);
                if choice.as_ref().is_none_or(|(_, best)| e < *best) {
                    choice = Some((band, e));
                }
            }
        }
        let Some((band, _)) = choice else { break };
        eq.bands[slot] = band;
        let spot = Spot::of(band.freq_hz, cut);
        placed.push((band, spot));
        spots.push((spot, band.freq_hz));
    }

    // Lowest band first, so the table reads low to high.
    let mut used: Vec<EqBand> = eq.bands[..3]
        .iter()
        .copied()
        .filter(|b| b.gain_db.abs() >= 0.05)
        .collect();
    used.sort_by(|a, b| a.freq_hz.total_cmp(&b.freq_hz));
    for (slot, band) in eq.bands[..3].iter_mut().enumerate() {
        *band = used.get(slot).copied().unwrap_or(defaults[slot]);
    }
    let eq = guardrails::soundcheck(input.role, &input.desk, &eq, input.feedback_hz);

    let end_err = weighted_rms(&error(input.measured_db, &target, &eq, &f), &f);
    if !hpf_changed && (placed.is_empty() || start_err - end_err < MIN_IMPROVEMENT_DB) {
        return None;
    }
    spots.sort_by(|a, b| a.1.total_cmp(&b.1));
    // Name the rumble last; it's the least interesting.
    spots.sort_by_key(|(s, _)| *s == Spot::Rumble);
    let changes = text::changes(&input.desk, &eq);
    if changes.is_empty() {
        return None;
    }
    Some(EqProposal {
        title: title(&spots),
        reason: sentence(&spots),
        eq,
        changes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyser::band_of;

    /// A speech LTAS that matches its target, plus `bumps` (hz, dB).
    fn speech_with(bumps: &[(f32, f32)]) -> Vec<f32> {
        let mut m = targets::target(ChannelRole::Speech, &Nudges::default()).unwrap();
        for (i, x) in m.iter_mut().enumerate() {
            let hz = band_hz(i);
            for &(c, db) in bumps {
                let d = (hz / c).log2() / 0.4;
                *x += db * (-d * d).exp();
            }
            *x -= 40.0;
        }
        m
    }

    fn input<'a>(measured: &'a [f32], nudges: &'a Nudges) -> SolveInput<'a> {
        SolveInput {
            role: ChannelRole::Speech,
            nudges,
            measured_db: measured,
            desk: ChannelEq::default(),
            feedback_hz: &[],
        }
    }

    #[test]
    fn cuts_a_boxy_lav() {
        let n = Nudges::default();
        let m = speech_with(&[(320.0, 6.0)]);
        let p = solve(&input(&m, &n)).expect("a proposal");
        let cut = p.eq.bands.iter().find(|b| b.gain_db < -1.0).expect("a cut");
        assert!((250.0..=420.0).contains(&cut.freq_hz), "{}", cut.freq_hz);
        assert!(cut.gain_db >= hard::TONE_MAX_CUT_DB);
        assert!(p.eq.hpf.on && p.eq.hpf.freq_hz <= 150.0);
        assert!(p.title.contains("boxy"), "{}", p.title);
        assert!(p.reason.starts_with("There's "), "{}", p.reason);
        // Band 4 is never part of a proposal.
        assert_eq!(p.eq.bands[3], ChannelEq::default().bands[3].snapped());
    }

    #[test]
    fn leaves_a_good_mic_alone_apart_from_the_low_cut() {
        let n = Nudges::default();
        let m = speech_with(&[]);
        let p = solve(&input(&m, &n)).expect("voices still get a low cut");
        assert!(p.eq.bands[..3].iter().all(|b| b.gain_db.abs() < 0.05));
        let mut desk = ChannelEq::default();
        desk.hpf.on = true;
        desk.hpf.freq_hz = 90.0;
        let mut i = input(&m, &n);
        i.desk = desk;
        assert!(solve(&i).is_none(), "nothing to change");
    }

    #[test]
    fn stays_inside_the_hard_limits() {
        let n = Nudges::default();
        let m = speech_with(&[(250.0, 20.0), (3_000.0, 18.0), (6_000.0, -20.0)]);
        let p = solve(&input(&m, &n)).unwrap();
        for b in &p.eq.bands[..3] {
            assert!(b.gain_db >= hard::TONE_MAX_CUT_DB - 0.01, "{b:?}");
            assert!(b.gain_db <= hard::MAX_BOOST_DB + 0.01, "{b:?}");
            if b.gain_db.abs() > 0.05 {
                assert!(
                    grid::width_value(b.width) <= hard::TONE_NARROWEST_WIDTH_VALUE,
                    "tone bands stay wide: {b:?}"
                );
            }
        }
    }

    #[test]
    fn never_boosts_near_a_feedback_frequency() {
        let n = Nudges::default();
        let m = speech_with(&[(4_000.0, -8.0)]);
        let mut i = input(&m, &n);
        let fb = [4_000.0];
        i.feedback_hz = &fb;
        if let Some(p) = solve(&i) {
            for b in &p.eq.bands {
                if b.gain_db > 0.05 {
                    assert!(!near_feedback(b.freq_hz, &fb), "{b:?}");
                }
            }
        }
    }

    #[test]
    fn keeps_a_person_s_low_cut() {
        let n = Nudges::default();
        let m = speech_with(&[(320.0, 6.0)]);
        let mut desk = ChannelEq::default();
        desk.hpf.on = true;
        desk.hpf.freq_hz = 180.0;
        let mut i = input(&m, &n);
        i.desk = desk;
        let p = solve(&i).unwrap();
        assert!(p.eq.hpf.on);
        assert_eq!(grid::hpf_value(p.eq.hpf.freq_hz), grid::hpf_value(180.0));
        assert!(band_of(p.eq.hpf.freq_hz).is_some());
    }

    #[test]
    fn leaves_playback_alone() {
        let n = Nudges::default();
        let m = speech_with(&[(320.0, 6.0)]);
        let mut i = input(&m, &n);
        i.role = ChannelRole::Playback;
        assert!(solve(&i).is_none());
    }
}
