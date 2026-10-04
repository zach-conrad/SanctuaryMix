//! Allen & Heath dLive adapter.
//!
//! dLive exposes its MIDI control protocol over plain TCP on the MixRack (or a
//! Surface) at port 51328. We speak that protocol directly, so no A&H MIDI
//! Control app or driver is needed on the Mac. Audio does NOT travel this way;
//! it arrives separately over Dante (see the `audio-engine` crate).

pub mod protocol;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use mix_core::{ChannelId, ConsoleEvent};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::TcpStream;
use tokio::sync::{broadcast, Mutex};
use tokio::task::JoinHandle;

use crate::{ConsoleAdapter, ConsoleConfig, ConsoleError, ConsoleModel, Result};
use protocol::{Decoder, MidiBase};

/// Unencrypted MIDI-over-TCP port on dLive MixRacks and Surfaces.
pub const DEFAULT_PORT: u16 = 51328;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

pub struct DliveAdapter {
    config: ConsoleConfig,
    base: MidiBase,
    writer: Option<Arc<Mutex<OwnedWriteHalf>>>,
    reader: Option<JoinHandle<()>>,
    connected: Arc<AtomicBool>,
    events: broadcast::Sender<ConsoleEvent>,
}

impl DliveAdapter {
    pub fn new(config: ConsoleConfig) -> Self {
        let (events, _) = broadcast::channel(1024);
        Self {
            base: MidiBase::new(config.midi_channel),
            config,
            writer: None,
            reader: None,
            connected: Arc::new(AtomicBool::new(false)),
            events,
        }
    }

    fn address(&self) -> String {
        format!(
            "{}:{}",
            self.config.host,
            self.config.port.unwrap_or(DEFAULT_PORT)
        )
    }

    async fn send(&self, bytes: &[u8]) -> Result<()> {
        let writer = self
            .writer
            .as_ref()
            .filter(|_| self.is_connected())
            .ok_or(ConsoleError::NotConnected)?;
        writer.lock().await.write_all(bytes).await?;
        Ok(())
    }
}

#[async_trait]
impl ConsoleAdapter for DliveAdapter {
    fn model(&self) -> ConsoleModel {
        ConsoleModel::Dlive
    }

    async fn connect(&mut self) -> Result<()> {
        self.disconnect().await;
        let address = self.address();
        let stream = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(&address))
            .await
            .map_err(|_| ConsoleError::Timeout(address.clone()))??;
        stream.set_nodelay(true)?;
        let (mut read_half, write_half) = stream.into_split();

        self.connected.store(true, Ordering::SeqCst);
        let connected = self.connected.clone();
        let events = self.events.clone();
        let base = self.base;
        self.reader = Some(tokio::spawn(async move {
            let mut decoder = Decoder::new(base);
            let mut buf = [0u8; 4096];
            let reason = loop {
                match read_half.read(&mut buf).await {
                    Ok(0) => break Some("console closed the connection".to_string()),
                    Ok(n) => {
                        for event in decoder.feed(&buf[..n]) {
                            let _ = events.send(event);
                        }
                    }
                    Err(e) => break Some(e.to_string()),
                }
            };
            connected.store(false, Ordering::SeqCst);
            let _ = events.send(ConsoleEvent::Disconnected { reason });
        }));
        self.writer = Some(Arc::new(Mutex::new(write_half)));
        log::info!("connected to dLive at {address}");
        let _ = self.events.send(ConsoleEvent::Connected {
            model: "Allen & Heath dLive".into(),
        });
        Ok(())
    }

    async fn disconnect(&mut self) {
        if let Some(reader) = self.reader.take() {
            reader.abort();
        }
        if let Some(writer) = self.writer.take() {
            let _ = writer.lock().await.shutdown().await;
        }
        if self.connected.swap(false, Ordering::SeqCst) {
            let _ = self
                .events
                .send(ConsoleEvent::Disconnected { reason: None });
        }
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    async fn set_fader(&self, id: ChannelId, db: Option<f32>) -> Result<()> {
        let msg = protocol::fader(self.base, id, db).ok_or(ConsoleError::UnsupportedChannel(id))?;
        self.send(&msg).await
    }

    async fn set_mute(&self, id: ChannelId, muted: bool) -> Result<()> {
        let msg =
            protocol::mute(self.base, id, muted).ok_or(ConsoleError::UnsupportedChannel(id))?;
        self.send(&msg).await
    }

    async fn request_name(&self, id: ChannelId) -> Result<()> {
        let msg =
            protocol::name_request(self.base, id).ok_or(ConsoleError::UnsupportedChannel(id))?;
        self.send(&msg).await
    }

    fn subscribe(&self) -> broadcast::Receiver<ConsoleEvent> {
        self.events.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ConsoleModel;
    use tokio::net::TcpListener;

    /// Stands up a fake MixRack on localhost and checks bytes go both ways.
    #[tokio::test]
    async fn talks_to_a_fake_mixrack() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let rack = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut got = [0u8; 6];
            sock.read_exact(&mut got).await.unwrap();
            // Console reports input 3 muted.
            sock.write_all(&[0x90, 0x02, 0x7F, 0x90, 0x02, 0x00])
                .await
                .unwrap();
            got
        });

        let mut desk = DliveAdapter::new(ConsoleConfig {
            model: ConsoleModel::Dlive,
            host: "127.0.0.1".into(),
            port: Some(port),
            midi_channel: 0,
            input_count: 8,
        });
        let mut rx = desk.subscribe();
        desk.connect().await.unwrap();
        desk.set_mute(ChannelId::input(0), true).await.unwrap();

        assert_eq!(rack.await.unwrap(), [0x90, 0x00, 0x7F, 0x90, 0x00, 0x00]);
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
    }

    #[tokio::test]
    async fn commands_fail_before_connect() {
        let desk = DliveAdapter::new(ConsoleConfig {
            model: ConsoleModel::Dlive,
            host: "127.0.0.1".into(),
            port: Some(1),
            midi_channel: 0,
            input_count: 8,
        });
        assert!(matches!(
            desk.set_mute(ChannelId::input(0), true).await,
            Err(ConsoleError::NotConnected)
        ));
    }
}
