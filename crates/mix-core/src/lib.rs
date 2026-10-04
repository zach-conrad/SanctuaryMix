//! Console-agnostic mixer model.
//!
//! Everything the UI, the audio engine and the console adapters agree on lives
//! here so that adding a new console (or a Windows build) never touches the
//! shared vocabulary. All types serialize to camelCase JSON for the frontend.

pub mod hearing;
pub mod level;

use serde::{Deserialize, Serialize};

/// The kind of processing channel on a console.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChannelKind {
    Input,
    Group,
    Aux,
    Matrix,
    Main,
    Dca,
    FxReturn,
}

/// A channel address, 0-based within its kind (input 1 on the surface is `index: 0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelId {
    pub kind: ChannelKind,
    pub index: u16,
}

impl ChannelId {
    pub const fn input(index: u16) -> Self {
        Self {
            kind: ChannelKind::Input,
            index,
        }
    }
}

/// The last known state of one console channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelState {
    pub id: ChannelId,
    pub name: String,
    /// Fader position in dB; `None` means fully down (-inf).
    pub fader_db: Option<f32>,
    pub muted: bool,
}

/// Instantaneous level of one audio input, in dBFS.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelMeter {
    pub channel: u16,
    pub peak_db: f32,
    pub rms_db: f32,
    /// True if any sample in the window reached full scale.
    pub clipped: bool,
}

/// One snapshot of every metered input, published to the UI ~30 times a second.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeterFrame {
    pub sample_rate: u32,
    pub channels: Vec<ChannelMeter>,
}

/// Something that changed on the console (from the console, or echoed back after we set it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ConsoleEvent {
    Connected { model: String },
    Disconnected { reason: Option<String> },
    Fader { id: ChannelId, db: Option<f32> },
    Mute { id: ChannelId, muted: bool },
    Name { id: ChannelId, name: String },
}
