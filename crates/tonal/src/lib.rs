//! AI EQ for SanctuaryMix.
//!
//! Hears each picked input's tone, proposes gentle dLive EQ at soundcheck,
//! catches feedback during the service and keeps speech mics sounding like
//! themselves. Docs: docs/AIEQ.md.
//!
//! - [`analyser`]: gated long-term spectrum and the feedback detector, on its
//!   own thread fed from the audio tap (like the listening models).
//! - [`targets`] and [`solver`]: what good sounds like per role, and the
//!   smallest desk EQ that gets there.
//! - [`guardrails`]: the hard limits every change passes, in Rust.
//! - [`engine`]: the controller, plain state with time passed in.
//! - [`runner`]: runs the engine against the console.
//! - [`model`]: the desk's EQ curves, shared with the UI's drawing.

pub mod analyser;
pub mod engine;
pub mod feedback;
pub mod guardrails;
pub mod model;
pub mod runner;
pub mod solver;
pub mod targets;
pub mod text;
pub mod types;

pub use types::*;
