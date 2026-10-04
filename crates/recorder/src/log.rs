//! The control-event log for one recording.
//!
//! Every [`ControlChange`] from the control bus goes through [`ControlLog::push`].
//! It turns the change into a [`RecordedEvent`] (time from the recording
//! start, next sequence number), folds console echoes of our own sends, and
//! buffers rows so the database sees a few small transactions a second
//! instead of one per fader tick.

use std::collections::VecDeque;
use std::fs;
use std::io::Write;
use std::path::Path;

use mix_core::{ChangeSource, ChannelId, ChannelState, ConsoleEvent, ControlChange};

use crate::{ChannelName, FileKind, RecordedEvent, Recording, RecordingStatus, Result, Store};

/// A console event this soon after our own send, with the same value, is its echo.
pub const ECHO_WINDOW_MS: u64 = 250;
/// Fader values this close count as the same value when folding echoes.
pub const ECHO_FADER_TOLERANCE_DB: f32 = 0.05;
/// Flush once the buffer holds more than this many events.
pub const FLUSH_MAX_EVENTS: usize = 200;
/// Flush once this long has passed since the last flush.
pub const FLUSH_INTERVAL_MS: u64 = 1_000;

/// Bundle file names.
pub const EVENTS_FILE: &str = "events.jsonl";
pub const MANIFEST_FILE: &str = "manifest.json";

#[derive(Debug, Clone, Copy)]
enum SentValue {
    Fader(Option<f32>),
    Mute(bool),
}

impl SentValue {
    fn matches(self, other: SentValue) -> bool {
        match (self, other) {
            (Self::Fader(None), Self::Fader(None)) => true,
            (Self::Fader(Some(a)), Self::Fader(Some(b))) => {
                (a - b).abs() <= ECHO_FADER_TOLERANCE_DB
            }
            (Self::Mute(a), Self::Mute(b)) => a == b,
            _ => false,
        }
    }
}

/// A recent change we sent to the console, waiting for its echo.
#[derive(Debug, Clone, Copy)]
struct Sent {
    at_ms: u64,
    channel: ChannelId,
    value: SentValue,
}

fn control_value(event: &ConsoleEvent) -> Option<(ChannelId, SentValue)> {
    match event {
        ConsoleEvent::Fader { id, db } => Some((*id, SentValue::Fader(*db))),
        ConsoleEvent::Mute { id, muted } => Some((*id, SentValue::Mute(*muted))),
        _ => None,
    }
}

pub struct ControlLog {
    recording_id: String,
    started_at_ms: u64,
    next_seq: u64,
    buffer: Vec<RecordedEvent>,
    sent: VecDeque<Sent>,
    last_flush_ms: u64,
}

impl ControlLog {
    /// A log for `recording_id`, which started at `started_at_ms` (epoch ms).
    pub fn new(recording_id: impl Into<String>, started_at_ms: u64) -> Self {
        Self {
            recording_id: recording_id.into(),
            started_at_ms,
            next_seq: 0,
            buffer: Vec::new(),
            sent: VecDeque::new(),
            last_flush_ms: started_at_ms,
        }
    }

    pub fn recording_id(&self) -> &str {
        &self.recording_id
    }

    /// Events waiting to be written.
    pub fn buffered(&self) -> usize {
        self.buffer.len()
    }

    /// Records the console state at the start as `snapshot` events at t=0
    /// (name, fader and mute per channel), stores the channel names, and
    /// writes them straight away.
    pub fn snapshot(&mut self, store: &Store, channels: &[ChannelState]) -> Result<()> {
        for ch in channels {
            for event in [
                ConsoleEvent::Name {
                    id: ch.id,
                    name: ch.name.clone(),
                },
                ConsoleEvent::Fader {
                    id: ch.id,
                    db: ch.fader_db,
                },
                ConsoleEvent::Mute {
                    id: ch.id,
                    muted: ch.muted,
                },
            ] {
                self.buffer_event(0, ChangeSource::Snapshot, event);
            }
        }
        let names: Vec<ChannelName> = channels
            .iter()
            .map(|c| ChannelName {
                id: c.id,
                name: c.name.clone(),
            })
            .collect();
        store.set_channel_names(&self.recording_id, &names)?;
        self.flush(store, self.started_at_ms)?;
        Ok(())
    }

    /// Adds one change to the buffer. Returns false when it was dropped as the
    /// console's echo of a change we sent. Does no IO; see [`Self::record`].
    pub fn push(&mut self, change: ControlChange) -> bool {
        let at = change.at_ms;
        while self
            .sent
            .front()
            .is_some_and(|s| s.at_ms.saturating_add(ECHO_WINDOW_MS) < at)
        {
            self.sent.pop_front();
        }
        if let Some((channel, value)) = control_value(&change.event) {
            match change.source {
                ChangeSource::Operator | ChangeSource::Assist | ChangeSource::Replay => {
                    self.sent.push_back(Sent {
                        at_ms: at,
                        channel,
                        value,
                    });
                }
                ChangeSource::Console => {
                    let is_echo = self.sent.iter().any(|s| {
                        s.channel == channel
                            && s.at_ms.abs_diff(at) <= ECHO_WINDOW_MS
                            && s.value.matches(value)
                    });
                    if is_echo {
                        return false;
                    }
                }
                ChangeSource::Snapshot => {}
            }
        }
        let t_ms = at.saturating_sub(self.started_at_ms);
        self.buffer_event(t_ms, change.source, change.event);
        true
    }

    /// [`Self::push`], then a flush if one is due (using the change's time as now).
    pub fn record(&mut self, store: &Store, change: ControlChange) -> Result<bool> {
        let now = change.at_ms;
        let kept = self.push(change);
        self.flush_if_due(store, now)?;
        Ok(kept)
    }

    /// True when the buffer is large or the last flush is over a second old.
    pub fn needs_flush(&self, now_ms: u64) -> bool {
        !self.buffer.is_empty()
            && (self.buffer.len() > FLUSH_MAX_EVENTS
                || now_ms.saturating_sub(self.last_flush_ms) > FLUSH_INTERVAL_MS)
    }

    /// Flushes if [`Self::needs_flush`]. Call this on a timer too, so a quiet
    /// stretch still gets written. Returns how many events were written.
    pub fn flush_if_due(&mut self, store: &Store, now_ms: u64) -> Result<usize> {
        if self.needs_flush(now_ms) {
            self.flush(store, now_ms)
        } else {
            Ok(0)
        }
    }

    /// Writes every buffered event in one transaction. On error the events
    /// stay buffered for the next try.
    pub fn flush(&mut self, store: &Store, now_ms: u64) -> Result<usize> {
        let n = self.buffer.len();
        if n > 0 {
            store.append_events(&self.recording_id, &self.buffer)?;
            self.buffer.clear();
        }
        self.last_flush_ms = now_ms;
        Ok(n)
    }

    /// Flushes, closes the row and writes `events.jsonl` and `manifest.json`.
    pub fn finish(
        mut self,
        store: &Store,
        ended_at_ms: u64,
        status: RecordingStatus,
    ) -> Result<Recording> {
        self.flush(store, ended_at_ms)?;
        store.finish_recording(&self.recording_id, ended_at_ms, status)?;
        finalize_bundle(store, &self.recording_id)
    }

    fn buffer_event(&mut self, t_ms: u64, source: ChangeSource, event: ConsoleEvent) {
        self.buffer.push(RecordedEvent {
            seq: self.next_seq,
            t_ms,
            source,
            event,
        });
        self.next_seq += 1;
    }
}

/// Writes `events.jsonl` and `manifest.json` into a recording's bundle and
/// registers both with their size and hash. Safe to call again (for example
/// for a recording recovered as `interrupted`). Returns the row as written.
pub fn finalize_bundle(store: &Store, id: &str) -> Result<Recording> {
    let dir = store.recording_dir(id);
    fs::create_dir_all(&dir)?;

    let mut jsonl = Vec::new();
    for event in store.events(id)? {
        serde_json::to_writer(&mut jsonl, &event)?;
        jsonl.push(b'\n');
    }
    write_atomic(&dir.join(EVENTS_FILE), &jsonl)?;
    let file = store.add_file(id, FileKind::Events, None, EVENTS_FILE)?;
    store.finalize_file(&file)?;

    let manifest = store.recording(id)?;
    write_atomic(
        &dir.join(MANIFEST_FILE),
        &serde_json::to_vec_pretty(&manifest)?,
    )?;
    let file = store.add_file(id, FileKind::Manifest, None, MANIFEST_FILE)?;
    store.finalize_file(&file)?;
    Ok(manifest)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AudioMode, NewRecording};

    fn setup() -> (tempfile::TempDir, Store, String) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let rec = store
            .create_recording(NewRecording {
                org_id: None,
                created_by: "local".into(),
                title: "Sunday".into(),
                service_date: "2026-10-04".into(),
                started_at: 10_000,
                console_model: "simulated".into(),
                sample_rate: None,
                audio_mode: AudioMode::None,
                mix_channels: None,
                track_count: 0,
            })
            .unwrap();
        (dir, store, rec.id)
    }

    fn change(at_ms: u64, source: ChangeSource, event: ConsoleEvent) -> ControlChange {
        ControlChange {
            at_ms,
            source,
            event,
        }
    }

    fn fader(ch: u16, db: f32) -> ConsoleEvent {
        ConsoleEvent::Fader {
            id: ChannelId::input(ch),
            db: Some(db),
        }
    }

    #[test]
    fn folds_echoes_of_our_sends() {
        let mut log = ControlLog::new("r", 10_000);
        assert!(log.push(change(10_100, ChangeSource::Operator, fader(0, -10.0))));
        // Echo: same channel, within tolerance and window.
        assert!(!log.push(change(10_200, ChangeSource::Console, fader(0, -10.03))));
        // Different value: a real desk move.
        assert!(log.push(change(10_210, ChangeSource::Console, fader(0, -4.0))));
        // Different channel.
        assert!(log.push(change(10_220, ChangeSource::Console, fader(1, -10.0))));
        // Too late to be the echo.
        assert!(log.push(change(10_400, ChangeSource::Console, fader(0, -10.0))));

        let mute = ConsoleEvent::Mute {
            id: ChannelId::input(2),
            muted: true,
        };
        assert!(log.push(change(11_000, ChangeSource::Assist, mute.clone())));
        assert!(!log.push(change(11_050, ChangeSource::Console, mute.clone())));
        let unmute = ConsoleEvent::Mute {
            id: ChannelId::input(2),
            muted: false,
        };
        assert!(log.push(change(11_060, ChangeSource::Console, unmute)));

        // Names and connection events are always kept.
        let name = ConsoleEvent::Name {
            id: ChannelId::input(0),
            name: "Vox".into(),
        };
        assert!(log.push(change(11_070, ChangeSource::Operator, name.clone())));
        assert!(log.push(change(11_080, ChangeSource::Console, name)));

        assert_eq!(log.buffered(), 8);
        assert_eq!(log.buffer[0].t_ms, 100);
        assert_eq!(log.buffer[7].seq, 7);
    }

    #[test]
    fn time_before_start_saturates() {
        let mut log = ControlLog::new("r", 10_000);
        log.push(change(9_000, ChangeSource::Console, fader(0, 0.0)));
        assert_eq!(log.buffer[0].t_ms, 0);
    }

    #[test]
    fn flushes_in_batches() {
        let (_dir, store, id) = setup();
        let mut log = ControlLog::new(id.clone(), 10_000);
        for i in 0..FLUSH_MAX_EVENTS as u64 {
            log.record(
                &store,
                change(10_000 + i, ChangeSource::Console, fader(0, i as f32 * -0.1)),
            )
            .unwrap();
        }
        assert_eq!(log.buffered(), FLUSH_MAX_EVENTS, "not due yet");
        log.record(&store, change(10_300, ChangeSource::Console, fader(0, 1.0)))
            .unwrap();
        assert_eq!(log.buffered(), 0, "flushed on size");
        assert_eq!(store.events(&id).unwrap().len(), FLUSH_MAX_EVENTS + 1);

        log.push(change(10_400, ChangeSource::Console, fader(0, 2.0)));
        assert!(!log.needs_flush(11_000));
        assert!(log.needs_flush(11_400));
        assert_eq!(log.flush_if_due(&store, 11_400).unwrap(), 1);
    }

    #[test]
    fn snapshot_and_finish_write_bundle() {
        let (_dir, store, id) = setup();
        let mut log = ControlLog::new(id.clone(), 10_000);
        let states = vec![
            ChannelState {
                id: ChannelId::input(0),
                name: "Pastor".into(),
                fader_db: Some(-3.0),
                muted: false,
            },
            ChannelState {
                id: ChannelId::input(1),
                name: "Choir".into(),
                fader_db: None,
                muted: true,
            },
        ];
        log.snapshot(&store, &states).unwrap();
        log.push(change(12_000, ChangeSource::Console, fader(0, -6.0)));
        let rec = log
            .finish(&store, 70_000, RecordingStatus::Complete)
            .unwrap();
        assert_eq!(rec.duration_ms, 60_000);

        let events = store.events(&id).unwrap();
        assert_eq!(events.len(), 7);
        assert!(events[..6]
            .iter()
            .all(|e| e.t_ms == 0 && e.source == ChangeSource::Snapshot));
        assert_eq!(events[6].t_ms, 2_000);

        let dir = store.recording_dir(&id);
        let jsonl = fs::read_to_string(dir.join(EVENTS_FILE)).unwrap();
        assert_eq!(jsonl.lines().count(), 7);
        let manifest: Recording =
            serde_json::from_slice(&fs::read(dir.join(MANIFEST_FILE)).unwrap()).unwrap();
        assert_eq!(manifest.id, id);
        assert_eq!(manifest.status, RecordingStatus::Complete);

        let detail = store.get_recording(&id).unwrap();
        assert_eq!(detail.channel_names.len(), 2);
        assert_eq!(detail.summary.event_count, 1);
        let kinds: Vec<FileKind> = detail.files.iter().map(|f| f.kind).collect();
        assert!(kinds.contains(&FileKind::Events) && kinds.contains(&FileKind::Manifest));
        assert!(detail.summary.bytes_on_disk > 0);

        // Calling it again is fine.
        finalize_bundle(&store, &id).unwrap();
        assert_eq!(store.files(&id).unwrap().len(), 2);
    }
}
