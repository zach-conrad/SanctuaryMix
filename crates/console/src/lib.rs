//! Console control.
//!
//! Each supported desk implements [`ConsoleAdapter`]. The app only ever talks
//! to the trait, so adding a Yamaha, DiGiCo or Behringer adapter later is a new
//! module plus one arm in [`create_adapter`].

pub mod dlive;
pub mod simulated;

use async_trait::async_trait;
use mix_core::eq::EqChange;
use mix_core::{ChannelId, ConsoleEvent};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

#[derive(Debug, thiserror::Error)]
pub enum ConsoleError {
    #[error("not connected to a console")]
    NotConnected,
    #[error("{0:?} is not addressable on this console")]
    UnsupportedChannel(ChannelId),
    #[error("network error: {0}")]
    Io(#[from] std::io::Error),
    #[error("timed out connecting to {0}")]
    Timeout(String),
    #[error("this console can't {0} from SanctuaryMix")]
    Unsupported(&'static str),
}

pub type Result<T> = std::result::Result<T, ConsoleError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConsoleModel {
    /// Allen & Heath dLive (MixRack or Surface), MIDI over TCP.
    Dlive,
    /// A fake desk for demos, UI work and tests. No hardware needed.
    Simulated,
}

/// How to reach a console. Serialized from the Setup screen.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleConfig {
    pub model: ConsoleModel,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: Option<u16>,
    /// dLive "MIDI channel" setting (1-12 on the console, stored here as 0-11).
    #[serde(default)]
    pub midi_channel: u8,
    /// How many input channels to show and query names for.
    #[serde(default = "default_input_count")]
    pub input_count: u16,
}

fn default_input_count() -> u16 {
    32
}

#[async_trait]
pub trait ConsoleAdapter: Send + Sync {
    fn model(&self) -> ConsoleModel;
    async fn connect(&mut self) -> Result<()>;
    async fn disconnect(&mut self);
    fn is_connected(&self) -> bool;
    async fn set_fader(&self, id: ChannelId, db: Option<f32>) -> Result<()>;
    async fn set_mute(&self, id: ChannelId, muted: bool) -> Result<()>;
    /// Ask the console for a channel name; the answer arrives as [`ConsoleEvent::Name`].
    async fn request_name(&self, id: ChannelId) -> Result<()>;
    /// Ask the console where a fader sits; the answer arrives as [`ConsoleEvent::Fader`].
    /// Consoles that can't be asked report faders only when they move.
    async fn request_fader(&self, _id: ChannelId) -> Result<()> {
        Ok(())
    }
    /// Whether input EQ (high-pass and parametric bands) can be read and set.
    fn supports_eq(&self) -> bool {
        false
    }
    /// Sets one EQ parameter on an input. Confirmed as [`ConsoleEvent::Eq`].
    async fn set_eq(&self, _id: ChannelId, _change: EqChange) -> Result<()> {
        Err(ConsoleError::Unsupported("change EQ"))
    }
    /// Asks for every EQ parameter on an input; answers arrive as [`ConsoleEvent::Eq`].
    async fn request_eq(&self, _id: ChannelId) -> Result<()> {
        Ok(())
    }
    /// Every change the console reports. Lagging receivers drop old events.
    fn subscribe(&self) -> broadcast::Receiver<ConsoleEvent>;
}

pub fn create_adapter(config: ConsoleConfig) -> Box<dyn ConsoleAdapter> {
    match config.model {
        ConsoleModel::Dlive => Box::new(dlive::DliveAdapter::new(config)),
        ConsoleModel::Simulated => Box::new(simulated::SimulatedConsole::new(config.input_count)),
    }
}
