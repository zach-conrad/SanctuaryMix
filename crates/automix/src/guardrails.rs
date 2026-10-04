//! The rules every auto-mix move must pass.
//!
//! [`Guardrails::limit`] is the single choke point between what the controller
//! would like and what reaches the console. It caps the size and speed of each
//! move, keeps every fader inside a range around where the operator left it,
//! never above unity unless the operator was already above it, and refuses to
//! push up a channel that is near clipping. Settings coming from the UI pass
//! through [`Guardrails::sanitized`], and `limit` sanitizes again, so no
//! setting can loosen the hard limits.
//!
//! Defaults follow the church live-mix research, section 5.

use serde::{Deserialize, Serialize};

/// dLive faders move in 0.5 dB steps, so smaller moves don't reach the desk.
pub const CONSOLE_STEP_DB: f32 = 0.5;

/// Values a setting can never go beyond, whatever the UI sends.
pub mod hard {
    /// Largest single move.
    pub const MAX_STEP_DB: f32 = 1.0;
    /// Fastest sustained movement.
    pub const MAX_RATE_DB_PER_SEC: f32 = 3.0;
    /// Furthest above the operator's position.
    pub const MAX_BOOST_DB: f32 = 8.0;
    /// Furthest below the operator's position.
    pub const MAX_CUT_DB: f32 = 12.0;
    /// Smallest dead band, so it can't chase every syllable.
    pub const MIN_DEADBAND_DB: f32 = 0.5;
    /// Auto-mix never takes a fader above unity unless the operator already had it there.
    pub const UNITY_DB: f32 = 0.0;
    /// The top of a dLive fader.
    pub const ABSOLUTE_CEILING_DB: f32 = 10.0;
    /// Lowest it will ever set. It never pulls a fader to −∞; that's a human action.
    pub const ABSOLUTE_FLOOR_DB: f32 = -60.0;
}

/// Limits for one kind of channel.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RideLimits {
    /// Largest single fader move.
    pub max_step_db: f32,
    /// Fastest the fader may travel, averaged over time.
    pub max_rate_db_per_sec: f32,
    /// Leave the fader alone while the level is within this much of its target.
    pub deadband_db: f32,
    /// How far above the operator's position a fader may go.
    pub max_boost_db: f32,
    /// How far below the operator's position a fader may go.
    pub max_cut_db: f32,
}

impl RideLimits {
    pub const MUSIC: RideLimits = RideLimits {
        max_step_db: 0.5,
        max_rate_db_per_sec: 1.5,
        deadband_db: 1.0,
        max_boost_db: 6.0,
        max_cut_db: 12.0,
    };

    /// Lavs vary the most week to week, so speech gets a wider, quicker range.
    pub const SPEECH: RideLimits = RideLimits {
        max_step_db: 1.0,
        max_rate_db_per_sec: 3.0,
        deadband_db: 1.5,
        max_boost_db: 8.0,
        max_cut_db: 10.0,
    };

    fn sanitized(self, fallback: RideLimits) -> Self {
        let c =
            |v: f32, lo: f32, hi: f32, fb: f32| if v.is_finite() { v.clamp(lo, hi) } else { fb };
        Self {
            max_step_db: c(
                self.max_step_db,
                CONSOLE_STEP_DB,
                hard::MAX_STEP_DB,
                fallback.max_step_db,
            ),
            max_rate_db_per_sec: c(
                self.max_rate_db_per_sec,
                0.25,
                hard::MAX_RATE_DB_PER_SEC,
                fallback.max_rate_db_per_sec,
            ),
            deadband_db: c(
                self.deadband_db,
                hard::MIN_DEADBAND_DB,
                6.0,
                fallback.deadband_db,
            ),
            max_boost_db: c(
                self.max_boost_db,
                0.0,
                hard::MAX_BOOST_DB,
                fallback.max_boost_db,
            ),
            max_cut_db: c(self.max_cut_db, 0.0, hard::MAX_CUT_DB, fallback.max_cut_db),
        }
    }

    /// The range a fader may move in around the operator's position.
    pub fn window(&self, baseline_db: f32) -> (f32, f32) {
        let ceiling = if baseline_db > hard::UNITY_DB {
            hard::ABSOLUTE_CEILING_DB
        } else {
            hard::UNITY_DB
        };
        let lo = (baseline_db - self.max_cut_db).max(hard::ABSOLUTE_FLOOR_DB);
        let hi = (baseline_db + self.max_boost_db).min(ceiling);
        (lo, hi.max(lo))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Guardrails {
    pub music: RideLimits,
    pub speech: RideLimits,
    /// Below this input level (RMS dBFS) a channel counts as idle and is never raised.
    pub gate_dbfs: f32,
    /// Never raise a fader while its input peaks above this (dBFS).
    pub no_raise_above_peak_dbfs: f32,
}

impl Default for Guardrails {
    fn default() -> Self {
        Self {
            music: RideLimits::MUSIC,
            speech: RideLimits::SPEECH,
            gate_dbfs: -50.0,
            no_raise_above_peak_dbfs: -3.0,
        }
    }
}

/// Everything [`Guardrails::limit`] needs to know about one channel.
#[derive(Debug, Clone, Copy)]
pub struct MoveContext {
    pub speech: bool,
    /// Where the fader is now.
    pub current_db: f32,
    /// Where the operator left it.
    pub baseline_db: f32,
    /// Seconds since this channel's last auto move (large if never).
    pub secs_since_last_move: f32,
    /// Recent input peak, dBFS.
    pub peak_dbfs: f32,
}

/// The tightest rule that shaped a move. Shown in the activity log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Limit {
    Step,
    Rate,
    Range,
    Unity,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limited {
    pub to_db: f32,
    pub limited_by: Option<Limit>,
}

impl Guardrails {
    /// The same settings pulled inside the hard limits.
    pub fn sanitized(self) -> Self {
        let d = Guardrails::default();
        Self {
            music: self.music.sanitized(d.music),
            speech: self.speech.sanitized(d.speech),
            gate_dbfs: if self.gate_dbfs.is_finite() {
                self.gate_dbfs.clamp(-80.0, -20.0)
            } else {
                d.gate_dbfs
            },
            no_raise_above_peak_dbfs: if self.no_raise_above_peak_dbfs.is_finite() {
                self.no_raise_above_peak_dbfs.clamp(-20.0, -1.0)
            } else {
                d.no_raise_above_peak_dbfs
            },
        }
    }

    pub fn limits(&self, speech: bool) -> RideLimits {
        if speech {
            self.speech
        } else {
            self.music
        }
    }

    /// Turns a wished-for fader position into a safe next step, or `None` to leave it alone.
    pub fn limit(&self, ctx: &MoveContext, wanted_db: f32) -> Option<Limited> {
        let g = self.sanitized();
        let limits = g.limits(ctx.speech);
        if !wanted_db.is_finite() || !ctx.current_db.is_finite() || !ctx.baseline_db.is_finite() {
            return None;
        }
        let mut limited_by = None;
        let mut target = wanted_db;

        // 1. Stay inside the range around the operator's position, and at or under unity.
        let (lo, hi) = limits.window(ctx.baseline_db);
        if target > hi {
            target = hi;
            limited_by = Some(
                if hi <= hard::UNITY_DB && ctx.baseline_db + limits.max_boost_db > hi {
                    Limit::Unity
                } else {
                    Limit::Range
                },
            );
        } else if target < lo {
            target = lo;
            limited_by = Some(Limit::Range);
        }

        // 2. Never push up something that is already close to clipping.
        if target > ctx.current_db && ctx.peak_dbfs > g.no_raise_above_peak_dbfs {
            return None;
        }

        // Clamping must never turn a move around. A fader the operator left
        // outside the range only moves back toward it, and only if that is
        // the way the controller wanted to go anyway.
        if (target - ctx.current_db).signum() != (wanted_db - ctx.current_db).signum() {
            return None;
        }

        // 3. Cap the size of this step and the speed of travel.
        let delta = target - ctx.current_db;
        let rate_budget = limits.max_rate_db_per_sec * ctx.secs_since_last_move.max(0.0);
        let mut allowed = delta.abs();
        if allowed > limits.max_step_db {
            allowed = limits.max_step_db;
            limited_by = Some(Limit::Step);
        }
        if allowed > rate_budget {
            allowed = rate_budget;
            limited_by = Some(Limit::Rate);
        }

        // 4. Move in whole console steps so what we send is what the desk does.
        let steps = (allowed / CONSOLE_STEP_DB + 1e-4).floor();
        if steps < 1.0 {
            return None;
        }
        let to_db = ctx.current_db + delta.signum() * steps * CONSOLE_STEP_DB;
        // Rounding can't carry it past the target or outside the range.
        let to_db = if delta > 0.0 {
            to_db.min(target)
        } else {
            to_db.max(target)
        };
        if (to_db - ctx.current_db).abs() < CONSOLE_STEP_DB - 1e-3 {
            return None;
        }
        Some(Limited { to_db, limited_by })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(current: f32, baseline: f32) -> MoveContext {
        MoveContext {
            speech: false,
            current_db: current,
            baseline_db: baseline,
            secs_since_last_move: 10.0,
            peak_dbfs: -20.0,
        }
    }

    #[test]
    fn caps_each_step_by_channel_kind() {
        let g = Guardrails::default();
        let m = g.limit(&ctx(-10.0, -10.0), -4.0).unwrap();
        assert_eq!(m.to_db, -9.5);
        assert_eq!(m.limited_by, Some(Limit::Step));
        let mut speech = ctx(-10.0, -10.0);
        speech.speech = true;
        assert_eq!(g.limit(&speech, -4.0).unwrap().to_db, -9.0);
        assert_eq!(g.limit(&ctx(-10.0, -10.0), -16.0).unwrap().to_db, -10.5);
    }

    #[test]
    fn waits_for_the_rate_budget() {
        let g = Guardrails::default();
        let mut c = ctx(-10.0, -10.0);
        c.secs_since_last_move = 0.2; // 1.5 dB/s allows 0.3 dB: less than one console step
        assert_eq!(g.limit(&c, -5.0), None);
        c.secs_since_last_move = 0.34;
        assert_eq!(g.limit(&c, -5.0).unwrap().to_db, -9.5);
    }

    #[test]
    fn never_leaves_the_range_around_the_operator() {
        let g = Guardrails::default();
        // Music: −12 to +6 around a −20 baseline.
        assert_eq!(g.limit(&ctx(-14.0, -20.0), 0.0), None);
        assert_eq!(g.limit(&ctx(-32.0, -20.0), -50.0), None);
        // Speech: −10 to +8.
        let mut s = ctx(-12.0, -20.0);
        s.speech = true;
        assert_eq!(g.limit(&s, 0.0), None);
        s.current_db = -30.0;
        assert_eq!(g.limit(&s, -50.0), None);
    }

    #[test]
    fn stays_at_or_under_unity_unless_the_operator_was_above_it() {
        let g = Guardrails::default();
        let m = g.limit(&ctx(-0.5, -3.0), 3.0).unwrap();
        assert_eq!(m.to_db, 0.0);
        assert_eq!(m.limited_by, Some(Limit::Unity));
        assert_eq!(g.limit(&ctx(0.0, -3.0), 3.0), None);
        // Operator had it at +2: the range applies, up to the top of the fader.
        assert_eq!(g.limit(&ctx(2.0, 2.0), 5.0).unwrap().to_db, 2.5);
        assert_eq!(g.limit(&ctx(8.0, 2.0), 12.0), None);
        let mut s = ctx(9.5, 4.0);
        s.speech = true;
        assert_eq!(g.limit(&s, 15.0).unwrap().to_db, 10.0);
    }

    #[test]
    fn will_not_raise_a_hot_channel_but_will_lower_it() {
        let g = Guardrails::default();
        let mut c = ctx(-10.0, -10.0);
        c.peak_dbfs = -1.0;
        assert_eq!(g.limit(&c, -8.0), None);
        assert_eq!(g.limit(&c, -12.0).unwrap().to_db, -10.5);
    }

    #[test]
    fn a_fader_outside_the_range_only_moves_back_toward_it() {
        let g = Guardrails::default();
        // Operator pushed it to +3 with a −5 baseline (above unity): wanting more does nothing.
        assert_eq!(g.limit(&ctx(3.0, -5.0), 5.0), None);
        assert_eq!(g.limit(&ctx(3.0, -5.0), -2.0).unwrap().to_db, 2.5);
    }

    #[test]
    fn settings_cannot_loosen_the_hard_limits() {
        let wild = RideLimits {
            max_step_db: 20.0,
            max_rate_db_per_sec: 100.0,
            deadband_db: 0.0,
            max_boost_db: 40.0,
            max_cut_db: 90.0,
        };
        let g = Guardrails {
            music: wild,
            speech: wild,
            gate_dbfs: -200.0,
            no_raise_above_peak_dbfs: 0.0,
        };
        let s = g.sanitized();
        for l in [s.music, s.speech] {
            assert_eq!(l.max_step_db, hard::MAX_STEP_DB);
            assert_eq!(l.max_rate_db_per_sec, hard::MAX_RATE_DB_PER_SEC);
            assert_eq!(l.max_boost_db, hard::MAX_BOOST_DB);
            assert_eq!(l.max_cut_db, hard::MAX_CUT_DB);
            assert_eq!(l.deadband_db, hard::MIN_DEADBAND_DB);
        }
        assert_eq!(s.gate_dbfs, -80.0);
        assert_eq!(s.no_raise_above_peak_dbfs, -1.0);

        let nan = Guardrails {
            music: RideLimits {
                max_step_db: f32::NAN,
                ..RideLimits::MUSIC
            },
            ..Guardrails::default()
        }
        .sanitized();
        assert_eq!(nan.music.max_step_db, RideLimits::MUSIC.max_step_db);

        // Unsanitized settings are sanitized at the choke point too.
        let m = g.limit(&ctx(-20.0, -20.0), -5.0).unwrap();
        assert!(m.to_db - -20.0 <= hard::MAX_STEP_DB);
        // And the window can never reach past the hard range.
        let (lo, hi) = g.sanitized().music.window(-20.0);
        assert_eq!((lo, hi), (-32.0, -12.0));
    }

    #[test]
    fn never_sets_minus_infinity() {
        let g = Guardrails::default();
        let m = g.limit(&ctx(-59.5, -55.0), -200.0).unwrap();
        assert_eq!(m.to_db, -60.0);
        assert_eq!(g.limit(&ctx(-60.0, -55.0), -200.0), None);
    }

    #[test]
    fn steps_land_on_console_resolution() {
        let g = Guardrails::default();
        let m = g.limit(&ctx(-3.2, -3.2), -1.0).unwrap();
        assert!((m.to_db - -2.7).abs() < 1e-4, "{m:?}");
        // Never overshoots a target closer than one step.
        assert_eq!(g.limit(&ctx(-5.0, -5.0), -4.7), None);
    }
}
