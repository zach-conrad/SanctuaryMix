//! The control bus: every console change, whoever made it, in one stream.
//!
//! Commands publish what a person did ([`ChangeSource::Operator`]), the
//! auto-mix publishes [`ChangeSource::Assist`], a replay publishes
//! [`ChangeSource::Replay`], and the console forwarder publishes what the desk
//! reports ([`ChangeSource::Console`]). Recordings subscribe and fold the
//! desk's echo of our own sends into the send that caused it.

use std::collections::BTreeMap;
use std::sync::Mutex;

use mix_core::{ChangeSource, ChannelId, ChannelKind, ChannelState, ConsoleEvent, ControlChange};
use tokio::sync::broadcast;

pub struct ControlBus {
    tx: broadcast::Sender<ControlChange>,
    /// Last state the console confirmed, so a recording can start with a snapshot.
    mirror: Mutex<BTreeMap<(u8, u16), ChannelState>>,
}

impl Default for ControlBus {
    fn default() -> Self {
        let (tx, _) = broadcast::channel(4096);
        Self {
            tx,
            mirror: Mutex::new(BTreeMap::new()),
        }
    }
}

fn key(id: ChannelId) -> (u8, u16) {
    let kind = match id.kind {
        ChannelKind::Input => 0,
        ChannelKind::Group => 1,
        ChannelKind::Aux => 2,
        ChannelKind::Matrix => 3,
        ChannelKind::Main => 4,
        ChannelKind::Dca => 5,
        ChannelKind::FxReturn => 6,
    };
    (kind, id.index)
}

impl ControlBus {
    pub fn publish(&self, source: ChangeSource, event: ConsoleEvent) {
        if source == ChangeSource::Console {
            self.remember(&event);
        }
        // No subscribers (nothing recording) is fine.
        let _ = self.tx.send(ControlChange::now(source, event));
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ControlChange> {
        self.tx.subscribe()
    }

    /// Every channel the console has told us about, in channel order.
    pub fn snapshot(&self) -> Vec<ChannelState> {
        self.mirror.lock().unwrap().values().cloned().collect()
    }

    fn remember(&self, event: &ConsoleEvent) {
        let mut mirror = self.mirror.lock().unwrap();
        let (id, apply): (ChannelId, &dyn Fn(&mut ChannelState)) = match event {
            ConsoleEvent::Fader { id, db } => (*id, &|s| s.fader_db = *db),
            ConsoleEvent::Mute { id, muted } => (*id, &|s| s.muted = *muted),
            ConsoleEvent::Name { id, name } => (*id, &|s| s.name = name.clone()),
            // EQ lives with AI EQ, which keeps its own copy of the desk's EQ.
            ConsoleEvent::Eq { .. } => return,
            ConsoleEvent::Disconnected { .. } => return mirror.clear(),
            ConsoleEvent::Connected { .. } => return,
        };
        let state = mirror.entry(key(id)).or_insert_with(|| ChannelState {
            id,
            name: String::new(),
            fader_db: None,
            muted: false,
        });
        apply(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_follows_the_console_only() {
        let bus = ControlBus::default();
        let ch = ChannelId::input(3);
        bus.publish(
            ChangeSource::Console,
            ConsoleEvent::Name {
                id: ch,
                name: "Kick".into(),
            },
        );
        bus.publish(
            ChangeSource::Console,
            ConsoleEvent::Fader {
                id: ch,
                db: Some(-4.5),
            },
        );
        // Not confirmed by the desk yet, so not in the snapshot.
        bus.publish(
            ChangeSource::Operator,
            ConsoleEvent::Mute {
                id: ch,
                muted: true,
            },
        );
        assert_eq!(
            bus.snapshot(),
            vec![ChannelState {
                id: ch,
                name: "Kick".into(),
                fader_db: Some(-4.5),
                muted: false
            }]
        );
        bus.publish(
            ChangeSource::Console,
            ConsoleEvent::Disconnected { reason: None },
        );
        assert!(bus.snapshot().is_empty());
    }
}
