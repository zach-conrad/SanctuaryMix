//! A console that lives in memory. Lets the whole app run with no hardware.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use mix_core::eq::{ChannelEq, EqChange};
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
    /// Fader positions; every input starts at 0 dB.
    faders: Mutex<HashMap<ChannelId, Option<f32>>>,
    /// Input EQ; every input starts flat.
    eqs: Mutex<HashMap<ChannelId, ChannelEq>>,
    events: broadcast::Sender<ConsoleEvent>,
}

impl SimulatedConsole {
    pub fn new(input_count: u16) -> Self {
        let (events, _) = broadcast::channel(256);
        Self {
            connected: false,
            input_count,
            names: Mutex::new(HashMap::new()),
            faders: Mutex::new(HashMap::new()),
            eqs: Mutex::new(HashMap::new()),
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
        self.faders.lock().unwrap().insert(id, db);
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

    async fn request_fader(&self, id: ChannelId) -> Result<()> {
        self.check(id)?;
        let db = *self.faders.lock().unwrap().entry(id).or_insert(Some(0.0));
        self.emit(ConsoleEvent::Fader { id, db });
        Ok(())
    }

    fn supports_eq(&self) -> bool {
        true
    }

    async fn set_eq(&self, id: ChannelId, change: EqChange) -> Result<()> {
        self.check(id)?;
        if let EqChange::BandKind { band, kind } = change {
            if !kind.allowed_on(band) {
                return Err(ConsoleError::UnsupportedChannel(id));
            }
        }
        // Snap to the desk's grid, like a real dLive.
        let mut eq = self
            .eqs
            .lock()
            .unwrap()
            .get(&id)
            .copied()
            .unwrap_or_default();
        eq.apply(&change);
        let eq = eq.snapped();
        self.eqs.lock().unwrap().insert(id, eq);
        let change = snapped_change(&eq, change);
        self.emit(ConsoleEvent::Eq { id, change });
        Ok(())
    }

    async fn request_eq(&self, id: ChannelId) -> Result<()> {
        self.check(id)?;
        let eq = *self.eqs.lock().unwrap().entry(id).or_default();
        for change in ChannelEq::default().diff(&eq).into_iter().chain(full(&eq)) {
            self.emit(ConsoleEvent::Eq { id, change });
        }
        Ok(())
    }

    fn subscribe(&self) -> broadcast::Receiver<ConsoleEvent> {
        self.events.subscribe()
    }
}

/// The value `change` ended up with on the desk.
fn snapped_change(eq: &ChannelEq, change: EqChange) -> EqChange {
    let band = |b: u8| eq.bands[b as usize];
    match change {
        EqChange::BandKind { band: b, .. } => EqChange::BandKind {
            band: b,
            kind: band(b).kind,
        },
        EqChange::BandFreq { band: b, .. } => EqChange::BandFreq {
            band: b,
            hz: band(b).freq_hz,
        },
        EqChange::BandWidth { band: b, .. } => EqChange::BandWidth {
            band: b,
            width: band(b).width,
        },
        EqChange::BandGain { band: b, .. } => EqChange::BandGain {
            band: b,
            db: band(b).gain_db,
        },
        EqChange::HpfOn { .. } => EqChange::HpfOn { on: eq.hpf.on },
        EqChange::HpfFreq { .. } => EqChange::HpfFreq { hz: eq.hpf.freq_hz },
    }
}

/// Every parameter of `eq`, the way a full read reports it.
fn full(eq: &ChannelEq) -> Vec<EqChange> {
    let mut out = vec![
        EqChange::HpfFreq { hz: eq.hpf.freq_hz },
        EqChange::HpfOn { on: eq.hpf.on },
    ];
    for (i, b) in eq.bands.iter().enumerate() {
        let band = i as u8;
        out.extend([
            EqChange::BandKind { band, kind: b.kind },
            EqChange::BandFreq {
                band,
                hz: b.freq_hz,
            },
            EqChange::BandWidth {
                band,
                width: b.width,
            },
            EqChange::BandGain {
                band,
                db: b.gain_db,
            },
        ]);
    }
    out
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

    #[tokio::test]
    async fn keeps_eq_on_the_desk_grid() {
        let mut desk = SimulatedConsole::new(8);
        desk.connect().await.unwrap();
        let mut rx = desk.subscribe();
        let id = ChannelId::input(0);
        desk.set_eq(id, EqChange::BandGain { band: 1, db: -3.1 })
            .await
            .unwrap();
        let ConsoleEvent::Eq {
            change: EqChange::BandGain { db, .. },
            ..
        } = rx.recv().await.unwrap()
        else {
            panic!("expected an EQ event");
        };
        assert!((db + 3.1).abs() < 0.13, "{db}");
        desk.request_eq(id).await.unwrap();
        let mut eq = ChannelEq::default();
        while let Ok(ConsoleEvent::Eq { change, .. }) = rx.try_recv() {
            eq.apply(&change);
        }
        assert!((eq.bands[1].gain_db - db).abs() < 1e-6);
        let hp = mix_core::eq::EqBandKind::HighPass;
        assert!(desk
            .set_eq(id, EqChange::BandKind { band: 2, kind: hp })
            .await
            .is_err());
    }

    #[tokio::test]
    async fn reports_fader_positions() {
        let mut desk = SimulatedConsole::new(8);
        desk.connect().await.unwrap();
        let mut rx = desk.subscribe();
        let id = ChannelId::input(1);
        desk.request_fader(id).await.unwrap();
        assert_eq!(
            rx.recv().await.unwrap(),
            ConsoleEvent::Fader { id, db: Some(0.0) }
        );
        desk.set_fader(id, Some(-6.5)).await.unwrap();
        rx.recv().await.unwrap();
        desk.request_fader(id).await.unwrap();
        assert_eq!(
            rx.recv().await.unwrap(),
            ConsoleEvent::Fader { id, db: Some(-6.5) }
        );
    }
}
