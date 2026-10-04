//! Picks up recording bundles copied into the recordings folder.
//!
//! A bundle (`manifest.json`, `events.jsonl`, `mix.wav`, `tracks/`) is the
//! unit that will sync to the cloud, so the same code path brings in a bundle
//! restored from a backup, shared by another church member, or a test sample.
//! The database is rebuilt from the bundle; the bundle stays as it is, except
//! that a folder not named by its recording id is renamed to that id.

use std::fs;
use std::path::{Path, PathBuf};

use mix_core::{ChangeSource, ConsoleEvent};

use crate::log::{EVENTS_FILE, MANIFEST_FILE};
use crate::{
    ChannelName, FileKind, RecordedEvent, Recording, RecordingStatus, Result, Store, SyncState,
};

/// What a folder scan found.
#[derive(Debug, Default)]
pub struct ImportReport {
    pub imported: Vec<String>,
    /// Folders that couldn't be read, with the reason.
    pub skipped: Vec<(PathBuf, String)>,
}

/// Imports every bundle under the store's root that isn't in the database yet.
/// A bundle that can't be read is skipped and reported, so one bad folder
/// never hides the rest.
pub fn import_bundles(store: &Store) -> Result<ImportReport> {
    let mut report = ImportReport::default();
    for entry in fs::read_dir(store.root())? {
        let dir = entry?.path();
        if !dir.join(MANIFEST_FILE).is_file() {
            continue;
        }
        match import_bundle(store, &dir) {
            Ok(Some(id)) => report.imported.push(id),
            Ok(None) => {}
            Err(e) => report.skipped.push((dir, e.to_string())),
        }
    }
    Ok(report)
}

/// Imports one bundle folder. `Ok(None)` when that recording is already known.
pub fn import_bundle(store: &Store, dir: &Path) -> Result<Option<String>> {
    let mut rec: Recording = serde_json::from_slice(&fs::read(dir.join(MANIFEST_FILE))?)?;
    uuid::Uuid::parse_str(&rec.id).map_err(|_| {
        crate::RecorderError::Invalid(format!("{:?} is not a recording id", rec.id))
    })?;
    if store.recording(&rec.id).is_ok() {
        return Ok(None);
    }

    // The folder must be named by the id; move it there if it isn't.
    let home = store.recording_dir(&rec.id);
    if dir != home {
        if home.exists() {
            return Err(crate::RecorderError::Invalid(format!(
                "{} already exists",
                home.display()
            )));
        }
        fs::rename(dir, &home)?;
    }

    let events = read_events(&home)?;
    if rec.status == RecordingStatus::Recording {
        rec.status = RecordingStatus::Interrupted;
    }
    if rec.ended_at.is_none() {
        let last = events.last().map_or(0, |e| e.t_ms);
        rec.duration_ms = rec.duration_ms.max(last);
        rec.ended_at = Some(rec.started_at + rec.duration_ms);
    }
    rec.deleted_at = None;
    rec.bytes_on_disk = 0;
    rec.remote_key = None;
    rec.sync_state = if rec.org_id.is_some() {
        SyncState::Pending
    } else {
        SyncState::LocalOnly
    };
    store.insert_row(&rec)?;

    store.append_events(&rec.id, &events)?;
    let names: Vec<ChannelName> = events
        .iter()
        .filter(|e| e.source == ChangeSource::Snapshot)
        .filter_map(|e| match &e.event {
            ConsoleEvent::Name { id, name } => Some(ChannelName {
                id: *id,
                name: name.clone(),
            }),
            _ => None,
        })
        .collect();
    store.set_channel_names(&rec.id, &names)?;

    let mut files = vec![
        (FileKind::Manifest, None, MANIFEST_FILE.to_string()),
        (FileKind::Events, None, EVENTS_FILE.to_string()),
        (FileKind::Mix, None, "mix.wav".to_string()),
    ];
    if let Ok(tracks) = fs::read_dir(home.join("tracks")) {
        for track in tracks.flatten() {
            let name = track.file_name().to_string_lossy().into_owned();
            // tracks/in-01.wav is device input 0.
            let channel = name
                .strip_prefix("in-")
                .and_then(|n| n.strip_suffix(".wav"))
                .and_then(|n| n.parse::<u16>().ok())
                .and_then(|n| n.checked_sub(1));
            if let Some(channel) = channel {
                files.push((FileKind::Track, Some(channel), format!("tracks/{name}")));
            }
        }
    }
    for (kind, channel, rel) in files {
        if home.join(&rel).is_file() {
            let file = store.add_file(&rec.id, kind, channel, &rel)?;
            store.finalize_file(&file)?;
        }
    }
    Ok(Some(rec.id))
}

fn read_events(dir: &Path) -> Result<Vec<RecordedEvent>> {
    let Ok(text) = fs::read_to_string(dir.join(EVENTS_FILE)) else {
        return Ok(Vec::new());
    };
    let mut events = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<std::result::Result<Vec<RecordedEvent>, _>>()?;
    events.sort_by_key(|e| (e.t_ms, e.seq));
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AudioMode, ControlLog, NewRecording};
    use mix_core::{ChannelId, ChannelState, ControlChange};

    #[test]
    fn a_bundle_round_trips_into_a_fresh_store() {
        let a = tempfile::tempdir().unwrap();
        let src = Store::open(a.path()).unwrap();
        let rec = src
            .create_recording(NewRecording {
                org_id: None,
                created_by: "local".into(),
                title: "Sunday 9:00 AM".into(),
                service_date: "2026-10-04".into(),
                started_at: 1_000,
                console_model: "simulated".into(),
                sample_rate: None,
                audio_mode: AudioMode::None,
                mix_channels: None,
                track_count: 0,
            })
            .unwrap();
        let mut log = ControlLog::new(rec.id.clone(), 1_000);
        let ch = ChannelId::input(0);
        log.snapshot(
            &src,
            &[ChannelState {
                id: ch,
                name: "Pastor".into(),
                fader_db: Some(-5.0),
                muted: false,
            }],
        )
        .unwrap();
        log.push(ControlChange {
            at_ms: 3_000,
            source: ChangeSource::Operator,
            event: ConsoleEvent::Fader {
                id: ch,
                db: Some(-2.0),
            },
        });
        log.finish(&src, 61_000, RecordingStatus::Complete).unwrap();

        // Copy the bundle under a friendly name into another store's folder.
        let b = tempfile::tempdir().unwrap();
        let copied = b.path().join("sample-service");
        fs::create_dir_all(&copied).unwrap();
        for f in [MANIFEST_FILE, EVENTS_FILE] {
            fs::copy(src.recording_dir(&rec.id).join(f), copied.join(f)).unwrap();
        }
        let dst = Store::open(b.path()).unwrap();
        assert_eq!(import_bundles(&dst).unwrap().imported, vec![rec.id.clone()]);
        // Second scan finds nothing new.
        assert!(import_bundles(&dst).unwrap().imported.is_empty());

        let detail = dst.get_recording(&rec.id).unwrap();
        assert_eq!(detail.summary.title, "Sunday 9:00 AM");
        assert_eq!(detail.summary.duration_ms, 60_000);
        assert_eq!(detail.summary.event_count, 1);
        assert_eq!(detail.channel_names[0].name, "Pastor");
        assert_eq!(detail.files.len(), 2);
        assert!(dst.recording_dir(&rec.id).join(MANIFEST_FILE).is_file());
    }
}
