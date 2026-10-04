//! A console that lives in memory. Lets the whole app run with no hardware.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use mix_core::{ChannelId, ConsoleEvent};
use tokio::sync::broadcast;

use crate::{ConsoleAdapter, ConsoleError, ConsoleModel, Result};

const DEMO_NAMES: &[&str] = &[
    "Pastor",
    "Worship Ld",
    "BGV 1",
    "BGV 2",
    "Kick",
    "Snare",
    "Hat",
    "Tom 1",
    "Tom 2",
    "OH L",
    "OH R",
    "Bass DI",
    "Elec Gtr",
    "Acous Gtr",
    "Keys L",
    "Keys R",
    "Pad L",
    "Pad R",
    "Choir L",
    "Choir R",
    "Handheld 1",
    "Handheld 2",
    "Video L",
    "Video R",
    "Playback L",
    "Playback R",
    "Lapel 1",
    "Lapel 2",
    "Ambient L",
    "Ambient R",
    "Spare 1",
    "Spare 2",
];

pub struct SimulatedConsole {
    connected: bool,
    input_count: u16,
    names: Mutex<HashMap<ChannelId, String>>,
    events: broadcast::Sender<ConsoleEvent>,
}

impl SimulatedConsole {
    pub fn new(input_count: u16) -> Self {
        let (events, _) = broadcast::channel(256);
        Self {
            connected: false,
            input_count,
            names: Mutex::new(HashMap::new()),
            events,
        }
    }

    fn check(&self, id: ChannelId) -> Result<()> {
        if !self.connected {
            return Err(ConsoleError::NotConnected);
        }
        if id.index >= self.input_count {
            return Err(ConsoleError::UnsupportedChannel(id));
        }
        Ok(())
    }

    fn emit(&self, event: ConsoleEvent) {
        // No subscribers is fine.
        let _ = self.events.send(event);
    }
}

#[async_trait]
impl ConsoleAdapter for SimulatedConsole {
    fn model(&self) -> ConsoleModel {
        ConsoleModel::Simulated
    }

    async fn connect(&mut self) -> Result<()> {
        self.connected = true;
        self.emit(ConsoleEvent::Connected {
            model: "Practice console".into(),
        });
        Ok(())
    }

    async fn disconnect(&mut self) {
        self.connected = false;
        self.emit(ConsoleEvent::Disconnected { reason: None });
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    async fn set_fader(&self, id: ChannelId, db: Option<f32>) -> Result<()> {
        self.check(id)?;
        self.emit(ConsoleEvent::Fader { id, db });
        Ok(())
    }

    async fn set_mute(&self, id: ChannelId, muted: bool) -> Result<()> {
        self.check(id)?;
        self.emit(ConsoleEvent::Mute { id, muted });
        Ok(())
    }

    async fn request_name(&self, id: ChannelId) -> Result<()> {
        self.check(id)?;
        let name = self
            .names
            .lock()
            .unwrap()
            .entry(id)
            .or_insert_with(|| {
                DEMO_NAMES
                    .get(id.index as usize)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("Ch {}", id.index + 1))
            })
            .clone();
        self.emit(ConsoleEvent::Name { id, name });
        Ok(())
    }

    fn subscribe(&self) -> broadcast::Receiver<ConsoleEvent> {
        self.events.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn echoes_changes_as_events() {
        let mut desk = SimulatedConsole::new(8);
        let mut rx = desk.subscribe();
        desk.connect().await.unwrap();
        desk.set_mute(ChannelId::input(2), true).await.unwrap();
        desk.request_name(ChannelId::input(0)).await.unwrap();

        assert!(matches!(
            rx.recv().await.unwrap(),
            ConsoleEvent::Connected { .. }
        ));
        assert_eq!(
            rx.recv().await.unwrap(),
            ConsoleEvent::Mute {
                id: ChannelId::input(2),
                muted: true
            }
        );
        assert_eq!(
            rx.recv().await.unwrap(),
            ConsoleEvent::Name {
                id: ChannelId::input(0),
                name: "Pastor".into()
            }
        );
    }

    #[tokio::test]
    async fn rejects_commands_when_disconnected() {
        let desk = SimulatedConsole::new(8);
        assert!(matches!(
            desk.set_fader(ChannelId::input(0), Some(0.0)).await,
            Err(ConsoleError::NotConnected)
        ));
    }
}
