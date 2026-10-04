//! On-device listening for SanctuaryMix.
//!
//! Two small models run on the computer, never in the cloud, so they work
//! offline and add nothing to the running cost:
//!
//! - **Silero VAD** (MIT) says, every 32 ms, whether someone is talking or
//!   singing into a mic. Auto-mix uses it to tell a voice from the band
//!   bleeding into that mic.
//! - **YAMNet** (Apache-2.0) recognises the kind of sound (speech, singing,
//!   drums, bass, piano, organ...) about once a second. It powers role
//!   suggestions and backs up the voice detector on sung vocals.
//!
//! Both run with `tract`, a pure-Rust inference engine, so there is no native
//! runtime to ship on Mac or Windows. Listening runs on its own thread
//! ([`Listener`]) and drops audio rather than ever slowing the audio path.
//! The models only describe what they hear; every fader decision and every
//! limit stays in the `automix` crate.

pub mod listener;
pub mod mel;
pub mod models;
pub mod resample;

pub use listener::{HeardSummary, ListenTarget, Listener};
pub use models::Models;

#[derive(Debug, thiserror::Error)]
pub enum ListenError {
    #[error("listening model error: {0}")]
    Model(String),
}
