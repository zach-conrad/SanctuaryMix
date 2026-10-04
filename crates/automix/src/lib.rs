//! AI auto-mix for SanctuaryMix.
//!
//! The operator picks which console inputs auto-mix may ride, gives each a
//! role (speech, lead vocal, kick...), and picks how the room should feel.
//! Auto-mix then keeps each of those faders near its target level for that
//! feel, so the mix sounds the same week to week whoever is in the booth.
//!
//! Scope today is levels only (fader rides and speech-over-music balance).
//! EQ comes later. The rules that keep it safe live in [`guardrails`] and are
//! enforced here in the core, not in the UI:
//!
//! - Small, slow moves only: 0.5 dB steps at up to 1.5 dB per second on
//!   music, 1 dB steps at up to 3 dB per second on speech, never past the
//!   hard limits.
//! - Each fader stays within a range around where the operator left it
//!   (−12 to +6 dB on music, −10 to +8 dB on speech), never above unity unless
//!   the operator already had it there, and never down to −∞.
//! - Only the channels the operator selected are ever touched. Auto-mix can
//!   only move faders: it has no path to mutes, scenes or routing, and it
//!   leaves muted channels alone.
//! - It never raises a mic nobody is using (the main guard against
//!   feedback), never raises a channel peaking near full scale, and leaves a
//!   clipping input alone (that's a preamp problem, not a fader one).
//! - A person always wins: moving a managed fader hands that channel back to
//!   the operator, Freeze stops every move at once, and Undo puts a channel
//!   back where the operator had it.
//! - Every move and every takeover is logged with a plain-language reason.

pub mod engine;
pub mod guardrails;
pub mod preset;
pub mod runner;

pub use engine::{
    Adjustment, AdjustmentKind, AutoMix, AutoMixConfig, AutoMixStatus, ChannelMode, ChannelStatus,
    FaderMove, ManagedChannel,
};
pub use guardrails::{Guardrails, Limit, RideLimits};
pub use preset::{guess_role, ChannelRole, Nudges, Preset, RoomFeel};
pub use runner::{start, AutoMixHandle, FaderSink, Observer, Stopped};
