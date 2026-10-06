//! Pure replay helpers: rebuild mixer state at a time, and pick the moves a
//! replay clock should send next. No IO.
//!
//! Inputs are a recording's events in order (`t_ms`, then `seq`), starting
//! with its snapshot, as [`crate::Store::events`] returns them.

use std::collections::HashMap;

use mix_core::{ChangeSource, ChannelId, ChannelState, ConsoleEvent};

use crate::RecordedEvent;

/// The channel an event is about, if any.
pub fn channel_of(event: &ConsoleEvent) -> Option<ChannelId> {
    event.channel()
}

/// Mixer state at `t_ms`: the snapshot plus every event up to and including
/// `t_ms`. Channels keep the order they first appear in.
pub fn state_at(events: &[RecordedEvent], t_ms: u64) -> Vec<ChannelState> {
    let mut order: Vec<ChannelState> = Vec::new();
    let mut index: HashMap<ChannelId, usize> = HashMap::new();
    for e in events.iter().take_while(|e| e.t_ms <= t_ms) {
        let Some(id) = channel_of(&e.event) else {
            continue;
        };
        let i = *index.entry(id).or_insert_with(|| {
            order.push(ChannelState {
                id,
                name: String::new(),
                fader_db: None,
                muted: false,
            });
            order.len() - 1
        });
        let ch = &mut order[i];
        match &e.event {
            ConsoleEvent::Fader { db, .. } => ch.fader_db = *db,
            ConsoleEvent::Mute { muted, .. } => ch.muted = *muted,
            ConsoleEvent::Name { name, .. } => ch.name.clone_from(name),
            _ => {}
        }
    }
    order
}

/// Moves to send as a replay clock goes from `from_ms` to `to_ms`: events with
/// `from_ms < t_ms <= to_ms`: fader, mute and EQ changes (no snapshot rows,
/// names or connection events), optionally limited to `channels`.
pub fn moves_between<'a>(
    events: &'a [RecordedEvent],
    from_ms: u64,
    to_ms: u64,
    channels: Option<&'a [ChannelId]>,
) -> impl Iterator<Item = &'a RecordedEvent> + 'a {
    let start = events.partition_point(|e| e.t_ms <= from_ms);
    events[start..]
        .iter()
        .take_while(move |e| e.t_ms <= to_ms)
        .filter(move |e| {
            if e.source == ChangeSource::Snapshot {
                return false;
            }
            let id = match &e.event {
                ConsoleEvent::Fader { id, .. }
                | ConsoleEvent::Mute { id, .. }
                | ConsoleEvent::Eq { id, .. } => *id,
                _ => return false,
            };
            channels.is_none_or(|set| set.contains(&id))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(seq: u64, t_ms: u64, source: ChangeSource, event: ConsoleEvent) -> RecordedEvent {
        RecordedEvent {
            seq,
            t_ms,
            source,
            event,
        }
    }

    fn sample() -> Vec<RecordedEvent> {
        let a = ChannelId::input(0);
        let b = ChannelId::input(1);
        vec![
            ev(
                0,
                0,
                ChangeSource::Snapshot,
                ConsoleEvent::Name {
                    id: a,
                    name: "Pastor".into(),
                },
            ),
            ev(
                1,
                0,
                ChangeSource::Snapshot,
                ConsoleEvent::Fader {
                    id: a,
                    db: Some(-5.0),
                },
            ),
            ev(
                2,
                0,
                ChangeSource::Snapshot,
                ConsoleEvent::Mute {
                    id: a,
                    muted: false,
                },
            ),
            ev(
                3,
                0,
                ChangeSource::Snapshot,
                ConsoleEvent::Name {
                    id: b,
                    name: "Choir".into(),
                },
            ),
            ev(
                4,
                0,
                ChangeSource::Snapshot,
                ConsoleEvent::Fader { id: b, db: None },
            ),
            ev(
                5,
                0,
                ChangeSource::Snapshot,
                ConsoleEvent::Mute { id: b, muted: true },
            ),
            ev(
                6,
                1_000,
                ChangeSource::Console,
                ConsoleEvent::Fader {
                    id: a,
                    db: Some(0.0),
                },
            ),
            ev(
                7,
                1_500,
                ChangeSource::Console,
                ConsoleEvent::Connected {
                    model: "dlive".into(),
                },
            ),
            ev(
                8,
                2_000,
                ChangeSource::Operator,
                ConsoleEvent::Mute {
                    id: b,
                    muted: false,
                },
            ),
            ev(
                9,
                2_000,
                ChangeSource::Console,
                ConsoleEvent::Name {
                    id: b,
                    name: "Band".into(),
                },
            ),
            ev(
                10,
                3_000,
                ChangeSource::Assist,
                ConsoleEvent::Fader {
                    id: b,
                    db: Some(-12.0),
                },
            ),
        ]
    }

    #[test]
    fn state_at_applies_events_up_to_t() {
        let events = sample();
        let s = state_at(&events, 0);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].name, "Pastor");
        assert_eq!(s[0].fader_db, Some(-5.0));
        assert!(s[1].muted);

        let s = state_at(&events, 1_999);
        assert_eq!(s[0].fader_db, Some(0.0));
        assert!(s[1].muted);

        let s = state_at(&events, 2_000);
        assert!(!s[1].muted);
        assert_eq!(s[1].name, "Band");
        assert_eq!(s[1].fader_db, None);

        let s = state_at(&events, u64::MAX);
        assert_eq!(s[1].fader_db, Some(-12.0));
    }

    #[test]
    fn moves_between_is_half_open_and_filtered() {
        let events = sample();
        let seqs = |from, to, ch: Option<&[ChannelId]>| -> Vec<u64> {
            moves_between(&events, from, to, ch)
                .map(|e| e.seq)
                .collect()
        };
        // From before the start: snapshot rows are never sent.
        assert_eq!(seqs(0, 3_000, None), vec![6, 8, 10]);
        assert_eq!(seqs(1_000, 2_000, None), vec![8]);
        assert_eq!(seqs(2_000, 2_999, None), Vec::<u64>::new());
        let only_b = [ChannelId::input(1)];
        assert_eq!(seqs(0, 5_000, Some(&only_b)), vec![8, 10]);
    }
}
