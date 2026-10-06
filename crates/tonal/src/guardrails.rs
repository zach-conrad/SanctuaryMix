//! The rules every AI EQ change must pass, enforced here and not in the UI.
//!
//! Three choke points, one per kind of change, all with [`hard`] limits no
//! setting can loosen (behavior.md section 3 and architecture.md section 7):
//!
//! - [`soundcheck`]: a proposal before the service. Gentle and wide on bands
//!   1-3 (here 0-2), band 4 untouched, the high-pass under the role's
//!   ceiling and never switched off once a person switched it on, no boosts
//!   near a frequency that has rung.
//! - [`live_tone`]: speech tone keeping during the service. Speech mics only,
//!   gain only, 0.5 dB at a time, no more than 0.5 dB per 10 s per band,
//!   within ±3 dB of the soundcheck EQ for the whole service, and a cap on
//!   messages per channel per minute.
//! - [`notch`]: a feedback notch on band 4: narrowest width, −3, −6 then −9 dB.
//!
//! The runner also keeps the whole link under [`hard::MSGS_PER_SEC`] and the
//! engine never changes a channel's EQ within [`hard::AFTER_FADER_STEP`] of
//! an auto-mix fader step on it.

use std::time::Duration;

use automix::ChannelRole;
use mix_core::eq::{grid, ChannelEq, EqBand, EqBandKind, NOTCH_BAND};

use crate::targets;

/// Values nothing can go beyond.
pub mod hard {
    use std::time::Duration;

    /// Most a band may sit above where it was before AI EQ touched it.
    pub const MAX_BOOST_DB: f32 = 3.0;
    /// Most any band may sit above 0 dB, whoever set the baseline.
    pub const ABSOLUTE_MAX_GAIN_DB: f32 = 6.0;
    /// Deepest tone cut.
    pub const TONE_MAX_CUT_DB: f32 = -6.0;
    /// Deepest feedback notch.
    pub const NOTCH_MAX_CUT_DB: f32 = -9.0;
    /// Tone bands are at least this wide: desk width value 12 is 2/3 octave.
    pub const TONE_NARROWEST_WIDTH_VALUE: u8 = 12;
    /// One live tone move.
    pub const LIVE_STEP_DB: f32 = 0.5;
    /// At most one live move per band in this window (0.5 dB per 10 s).
    pub const LIVE_RATE_WINDOW: Duration = Duration::from_secs(10);
    /// Live tone stays within this of the soundcheck EQ, per band, all service.
    pub const LIVE_RANGE_DB: f32 = 3.0;
    /// EQ messages per channel per minute while live.
    pub const LIVE_MSGS_PER_MINUTE: usize = 6;
    /// Total EQ messages per second to the desk.
    pub const MSGS_PER_SEC: u32 = 20;
    /// No EQ change this soon after an auto-mix fader step on the same channel.
    pub const AFTER_FADER_STEP: Duration = Duration::from_secs(1);
}

/// No boost within this many octaves of a frequency that has rung (±1/6 octave).
pub const FEEDBACK_GUARD_OCTAVES: f32 = 1.0 / 6.0;

/// Notch depth for each stage of a ring that keeps going.
pub const NOTCH_STAGES_DB: [f32; 3] = [-3.0, -6.0, -9.0];

fn near(hz: f32, list: &[f32]) -> bool {
    list.iter()
        .any(|&f| f > 0.0 && (hz / f).log2().abs() <= FEEDBACK_GUARD_OCTAVES)
}

/// A soundcheck proposal made safe. `before` is the desk's EQ now.
pub fn soundcheck(
    role: ChannelRole,
    before: &ChannelEq,
    proposed: &ChannelEq,
    feedback_hz: &[f32],
) -> ChannelEq {
    let mut out = *proposed;
    // Band 4 belongs to feedback notches; soundcheck never touches it.
    out.bands[NOTCH_BAND as usize] = before.bands[NOTCH_BAND as usize];
    for (i, band) in out.bands.iter_mut().enumerate().take(NOTCH_BAND as usize) {
        let b4 = before.bands[i];
        if !band.kind.allowed_on(i as u8) || !band.gain_db.is_finite() || !band.freq_hz.is_finite()
        {
            *band = b4;
            continue;
        }
        let ceiling = (b4.gain_db.max(0.0) + hard::MAX_BOOST_DB).min(hard::ABSOLUTE_MAX_GAIN_DB);
        band.gain_db = grid::gain_within(band.gain_db, hard::TONE_MAX_CUT_DB, ceiling);
        if band.gain_db > 0.0 && near(band.freq_hz, feedback_hz) {
            band.gain_db = 0.0;
        }
        if band.kind == EqBandKind::Bell
            && band.gain_db.abs() >= 0.05
            && grid::width_value(band.width) > hard::TONE_NARROWEST_WIDTH_VALUE
        {
            band.width = grid::width_from_value(hard::TONE_NARROWEST_WIDTH_VALUE);
        }
    }
    // The high-pass: a person's stays as they set it; ours stays under the ceiling.
    if before.hpf.on {
        out.hpf = before.hpf;
    } else if out.hpf.on {
        match targets::hpf_rule(role) {
            Some(rule) if out.hpf.freq_hz.is_finite() => {
                let mut v = grid::hpf_value(out.hpf.freq_hz.min(rule.ceiling_hz));
                while v > 0 && grid::hpf_from_value(v) > rule.ceiling_hz {
                    v -= 1;
                }
                out.hpf.freq_hz = grid::hpf_from_value(v);
            }
            _ => out.hpf = before.hpf,
        }
    }
    // Snap everything else; gains are already on the grid inside their limits.
    let gains: Vec<f32> = out.bands.iter().map(|b| b.gain_db).collect();
    let mut out = out.snapped();
    for (b, g) in out.bands.iter_mut().zip(gains) {
        b.gain_db = g;
    }
    out
}

/// What [`live_tone`] needs about one band of one channel.
#[derive(Debug, Clone, Copy)]
pub struct ToneContext {
    pub role: ChannelRole,
    /// The band's gain at soundcheck (this service's baseline).
    pub baseline_db: f32,
    /// Its gain on the desk now.
    pub current_db: f32,
    /// Time since the last live move on this band (large if never).
    pub since_last_move: Duration,
    /// Live EQ messages sent to this channel in the last minute.
    pub msgs_last_minute: usize,
    /// Time since auto-mix last moved this channel's fader (large if never).
    pub since_fader_step: Duration,
}

/// The next live tone step toward `wanted_db`, or `None` to leave it.
pub fn live_tone(ctx: &ToneContext, wanted_db: f32) -> Option<f32> {
    if ctx.role != ChannelRole::Speech
        || !wanted_db.is_finite()
        || !ctx.current_db.is_finite()
        || !ctx.baseline_db.is_finite()
        || ctx.since_last_move < hard::LIVE_RATE_WINDOW
        || ctx.since_fader_step < hard::AFTER_FADER_STEP
        || ctx.msgs_last_minute >= hard::LIVE_MSGS_PER_MINUTE
    {
        return None;
    }
    let lo = (ctx.baseline_db - hard::LIVE_RANGE_DB).max(hard::TONE_MAX_CUT_DB);
    let hi = (ctx.baseline_db + hard::LIVE_RANGE_DB)
        .min(ctx.baseline_db.max(0.0) + hard::MAX_BOOST_DB)
        .min(hard::ABSOLUTE_MAX_GAIN_DB);
    let target = wanted_db.clamp(lo, hi.max(lo));
    let delta = target - ctx.current_db;
    // Clamping must never turn a move around.
    if delta.signum() != (wanted_db - ctx.current_db).signum() || delta.abs() < 0.2 {
        return None;
    }
    let to = ctx.current_db + delta.signum() * delta.abs().min(hard::LIVE_STEP_DB);
    // On the desk's grid, never past the target or a full step.
    let (lo_s, hi_s) = if delta > 0.0 {
        (ctx.current_db, to.min(target))
    } else {
        (to.max(target), ctx.current_db)
    };
    let snapped = grid::gain_within(to, lo_s, hi_s);
    (grid::gain_value(snapped) != grid::gain_value(ctx.current_db)).then_some(snapped)
}

/// The notch for `stage` (0, 1, 2) at `hz` on band 4.
pub fn notch(hz: f32, stage: usize) -> EqBand {
    let gain = NOTCH_STAGES_DB[stage.min(NOTCH_STAGES_DB.len() - 1)].max(hard::NOTCH_MAX_CUT_DB);
    let mut b = EqBand::bell(hz, grid::NARROWEST, gain).snapped();
    b.gain_db = grid::gain_within(gain, hard::NOTCH_MAX_CUT_DB, 0.0);
    b
}

/// Whether band 4 is free for a notch: a bell sitting at 0 dB.
pub fn notch_band_free(eq: &ChannelEq) -> bool {
    let b = eq.bands[NOTCH_BAND as usize];
    b.kind == EqBandKind::Bell && b.gain_db.abs() < 0.3
}

/// Whether band 4 already holds a notch near `hz` (the same ring again).
pub fn notch_matches(eq: &ChannelEq, hz: f32) -> bool {
    let b = eq.bands[NOTCH_BAND as usize];
    b.kind == EqBandKind::Bell && b.gain_db < -0.3 && (hz / b.freq_hz).log2().abs() <= 1.0 / 12.0
}

/// Live window: how long ago something happened, saturating.
pub fn since(now: Duration, then: Option<Duration>) -> Duration {
    then.map_or(Duration::MAX, |t| now.saturating_sub(t))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(current: f32, baseline: f32) -> ToneContext {
        ToneContext {
            role: ChannelRole::Speech,
            baseline_db: baseline,
            current_db: current,
            since_last_move: Duration::from_secs(60),
            msgs_last_minute: 0,
            since_fader_step: Duration::from_secs(60),
        }
    }

    #[test]
    fn live_tone_moves_half_a_db_at_a_time() {
        let to = live_tone(&tone(0.0, 0.0), -2.0).unwrap();
        assert!((to + 0.5).abs() < 0.13, "{to}");
        let to = live_tone(&tone(0.0, 0.0), 2.0).unwrap();
        assert!(to > 0.0 && to <= 0.5 + 1e-4, "{to}");
    }

    #[test]
    fn live_tone_stays_within_three_db_of_soundcheck() {
        assert_eq!(live_tone(&tone(-3.0, 0.0), -6.0), None);
        assert_eq!(live_tone(&tone(-3.0, 0.0), -3.5), None);
        let to = live_tone(&tone(-3.0, 0.0), -1.0).unwrap();
        assert!(to > -3.0);
    }

    #[test]
    fn live_tone_waits_and_only_touches_speech() {
        let mut c = tone(0.0, 0.0);
        c.since_last_move = Duration::from_secs(5);
        assert_eq!(live_tone(&c, -2.0), None);
        let mut c = tone(0.0, 0.0);
        c.role = ChannelRole::LeadVocal;
        assert_eq!(live_tone(&c, -2.0), None);
        let mut c = tone(0.0, 0.0);
        c.since_fader_step = Duration::from_millis(400);
        assert_eq!(live_tone(&c, -2.0), None);
        let mut c = tone(0.0, 0.0);
        c.msgs_last_minute = hard::LIVE_MSGS_PER_MINUTE;
        assert_eq!(live_tone(&c, -2.0), None);
    }

    #[test]
    fn soundcheck_proposals_are_gentle_and_leave_band_4() {
        let before = ChannelEq::default();
        let mut p = before;
        p.bands[0] = EqBand::bell(300.0, grid::NARROWEST, -12.0);
        p.bands[1] = EqBand::bell(3_000.0, 1.0, 9.0);
        p.bands[3] = EqBand::bell(2_500.0, grid::NARROWEST, -9.0);
        p.hpf.on = true;
        p.hpf.freq_hz = 400.0;
        let s = soundcheck(ChannelRole::Speech, &before, &p, &[]);
        assert!(s.bands[0].gain_db >= hard::TONE_MAX_CUT_DB - 0.01);
        assert!(grid::width_value(s.bands[0].width) <= hard::TONE_NARROWEST_WIDTH_VALUE);
        assert!(s.bands[1].gain_db <= hard::MAX_BOOST_DB + 0.01);
        assert_eq!(s.bands[3], before.snapped().bands[3]);
        assert!(s.hpf.freq_hz <= 150.0, "{}", s.hpf.freq_hz);
    }

    #[test]
    fn soundcheck_never_boosts_near_feedback_or_drops_a_person_s_low_cut() {
        let mut before = ChannelEq::default();
        before.hpf.on = true;
        before.hpf.freq_hz = 200.0;
        let mut p = before;
        p.bands[2] = EqBand::bell(2_500.0, 1.0, 2.0);
        p.hpf.on = false;
        let s = soundcheck(ChannelRole::Speech, &before, &p, &[2_600.0]);
        assert!(s.bands[2].gain_db.abs() < 0.13);
        assert!(s.hpf.on);
        assert_eq!(grid::hpf_value(s.hpf.freq_hz), grid::hpf_value(200.0));
    }

    #[test]
    fn notches_deepen_in_stages_and_stop_at_minus_nine() {
        assert!((notch(2_500.0, 0).gain_db + 3.0).abs() < 0.13);
        assert!((notch(2_500.0, 1).gain_db + 6.0).abs() < 0.13);
        // −9 dB is between two desk steps; the notch never goes past it.
        let deepest = notch(2_500.0, 7).gain_db;
        assert!((-9.0..-8.7).contains(&deepest), "{deepest}");
        assert_eq!(grid::width_value(notch(2_500.0, 0).width), 24);
    }

    #[test]
    fn band_4_ownership() {
        let mut eq = ChannelEq::default();
        assert!(notch_band_free(&eq));
        eq.bands[3] = notch(2_500.0, 0);
        assert!(!notch_band_free(&eq));
        assert!(notch_matches(&eq, 2_550.0));
        assert!(!notch_matches(&eq, 1_000.0));
    }
}
