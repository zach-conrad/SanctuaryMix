//! What the on-device listening models hear on each input.
//!
//! Produced by the `listen` crate, used by auto-mix and shown in the UI. Kept
//! here so auto-mix can use it without depending on the model runtime.

use serde::{Deserialize, Serialize};

/// The broad kind of sound on a mic, in the words a volunteer would use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Sound {
    Speech,
    Singing,
    Choir,
    Drums,
    Bass,
    ElectricGuitar,
    AcousticGuitar,
    Piano,
    Organ,
    Keys,
    Brass,
    Strings,
    /// Music, with no one instrument standing out.
    Music,
    /// Something else: noise, the room, applause.
    Other,
}

impl Sound {
    /// A person's voice, spoken or sung.
    pub fn is_voice(self) -> bool {
        matches!(self, Sound::Speech | Sound::Singing | Sound::Choir)
    }
}

/// One input's latest listening result.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hearing {
    /// Audio input, 0-based.
    pub channel: u16,
    /// Someone is talking or singing into this mic right now (not just
    /// picking up the band or another voice from across the stage).
    pub voice: bool,
    /// Voice-activity probability behind `voice`, 0 to 1.
    pub voice_prob: f32,
    /// What it mostly hears, once there's been enough signal to tell.
    pub sound: Option<Sound>,
    /// How sure the classifier is about `sound`, 0 to 1.
    pub confidence: f32,
}

/// Listening results for every input being listened to.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HearingFrame {
    pub channels: Vec<Hearing>,
}
