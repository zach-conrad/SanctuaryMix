//! Service recordings in the app: start/stop, the live control log, disk
//! guards and the commands the Services screen uses. Storage lives in the
//! `recorder` crate and audio capture rides the metering stream in
//! `audio-engine`; this module wires them to the control bus and the UI.
//! See docs/RECORDINGS.md.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use audio_engine::{AudioRecordOptions, AudioRecorder, WrittenFileKind as AudioFileKind};
use recorder::disk::{self, DiskAction, RecordPlan};
use recorder::{
    AudioMode, ControlLog, FileKind, NewRecording, RecordedEvent, RecordingDetail, RecordingStatus,
    RecordingSummary, RecordingSync, Store,
};
use serde::{Deserialize, Serialize};
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::{broadcast::error::RecvError, oneshot};

use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

const SETTINGS_KEY: &str = "recording.settings";
const TICK: Duration = Duration::from_secs(1);

/// What to capture besides the control moves. Saved in the app database.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingSettings {
    /// Dante inputs (0-based) carrying Main L/R.
    pub mix_channels: Option<[u16; 2]>,
    pub multitrack: bool,
}

/// Pushed to the UI as the `recording` event about once a second.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecorderStatus {
    pub active: Option<RecordingSummary>,
    pub elapsed_ms: u64,
    pub bytes_written: u64,
    pub free_bytes: u64,
    pub warning: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskUsage {
    free_bytes: u64,
    recordings_bytes: u64,
    multitrack_bytes: u64,
    folder: String,
}

/// The running recording, if any.
struct Active {
    id: String,
    stop: oneshot::Sender<()>,
    task: JoinHandle<CmdResult<RecordingSummary>>,
}

pub struct Recordings {
    pub store: Arc<Store>,
    pub sync: Box<dyn RecordingSync>,
    active: tokio::sync::Mutex<Option<Active>>,
    status: Arc<Mutex<RecorderStatus>>,
}

impl Recordings {
    /// Opens `<app data>/recordings`, closing anything a crash left open.
    pub fn open(app: &AppHandle) -> Result<Self, String> {
        let root = app.path().app_data_dir().map_err(err)?.join("recordings");
        std::fs::create_dir_all(&root).map_err(err)?;
        let store = Arc::new(Store::open(&root).map_err(err)?);
        for id in store.mark_interrupted_on_startup().map_err(err)? {
            log::warn!("recording {id} was interrupted; keeping what was saved");
            if let Err(e) = recorder::finalize_bundle(&store, &id) {
                log::error!("couldn't finish the bundle for {id}: {e}");
            }
        }
        // Bundles copied in (a backup, a shared service, a test sample).
        let report = recorder::import::import_bundles(&store).map_err(err)?;
        for id in &report.imported {
            log::info!("imported recording {id}");
        }
        for (dir, why) in &report.skipped {
            log::warn!("skipped recording folder {}: {why}", dir.display());
        }
        Ok(Self {
            store,
            sync: Box::new(recorder::LocalOnly),
            active: tokio::sync::Mutex::new(None),
            status: Arc::new(Mutex::new(RecorderStatus::default())),
        })
    }

    pub async fn is_recording(&self) -> bool {
        self.active.lock().await.is_some()
    }

    fn folder(&self) -> PathBuf {
        self.store.root().to_path_buf()
    }
}

fn recordings(state: &AppState) -> CmdResult<&Recordings> {
    state.recordings.as_ref().ok_or_else(|| {
        "Recordings aren't available: the recordings folder couldn't be opened.".into()
    })
}

fn load_settings(state: &AppState) -> RecordingSettings {
    state
        .store
        .lock()
        .unwrap()
        .get_setting(SETTINGS_KEY)
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// "Sunday 9:00 AM" and "2026-10-04" in the computer's time zone.
fn default_title_and_date() -> (String, String) {
    let now = chrono::Local::now();
    (
        now.format("%A %-I:%M %p").to_string(),
        now.format("%Y-%m-%d").to_string(),
    )
}

#[tauri::command]
pub async fn get_recording_settings(state: State<'_, AppState>) -> CmdResult<RecordingSettings> {
    Ok(load_settings(&state))
}

#[tauri::command]
pub async fn set_recording_settings(
    state: State<'_, AppState>,
    settings: RecordingSettings,
) -> CmdResult<()> {
    state
        .store
        .lock()
        .unwrap()
        .set_setting(SETTINGS_KEY, &settings)
        .map_err(err)
}

#[tauri::command]
pub async fn start_recording(
    app: AppHandle,
    state: State<'_, AppState>,
    title: Option<String>,
) -> CmdResult<RecordingSummary> {
    let recs = recordings(&state)?;
    let mut active = recs.active.lock().await;
    if active.is_some() {
        return Err("A service is already recording. Stop it first.".into());
    }

    let settings = load_settings(&state);
    let session = state.auth.current_session().await;
    let (org_id, created_by) = recorder::owner_from_session(&session);
    let console_model = match state.console.lock().await.as_ref() {
        Some(adapter) => serde_json::to_value(adapter.model())
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default(),
        None => "none".into(),
    };

    // Audio rides the metering stream; without it we still log every move.
    let (sample_rate, device_channels) = {
        let metering = state.metering.lock().unwrap();
        metering
            .as_ref()
            .map(|m| (Some(m.sample_rate), m.channels))
            .unwrap_or((None, 0))
    };
    let mix_channels = settings
        .mix_channels
        .filter(|[l, r]| *l < device_channels && *r < device_channels);
    let multitrack = settings.multitrack && sample_rate.is_some();
    let audio_mode = match (sample_rate, mix_channels, multitrack) {
        (None, _, _) | (_, None, false) => AudioMode::None,
        (_, _, true) => AudioMode::StereoMultitrack,
        _ => AudioMode::Stereo,
    };
    let track_count = if multitrack { device_channels } else { 0 };

    let free = disk::free_bytes(&recs.folder()).map_err(err)?;
    if audio_mode != AudioMode::None {
        disk::check_start(
            free,
            &RecordPlan {
                audio_mode,
                sample_rate: sample_rate.unwrap_or(48_000),
                track_count,
            },
        )?;
    }

    let (default_title, service_date) = default_title_and_date();
    let started_at = recorder::now_ms();
    let recording = recs
        .store
        .create_recording(NewRecording {
            org_id,
            created_by,
            title: title
                .filter(|t| !t.trim().is_empty())
                .unwrap_or(default_title),
            service_date,
            started_at,
            console_model,
            sample_rate: if audio_mode == AudioMode::None {
                None
            } else {
                sample_rate
            },
            audio_mode,
            mix_channels,
            track_count,
        })
        .map_err(err)?;
    let id = recording.id.clone();
    let dir = recs.store.recording_dir(&id);

    let audio = if audio_mode == AudioMode::None {
        None
    } else {
        let metering = state.metering.lock().unwrap();
        let handle = metering
            .as_ref()
            .ok_or("Dante audio stopped. Start listening in Setup, then record again.")?;
        let started = handle.start_recording(AudioRecordOptions {
            dir: dir.clone(),
            mix_channels,
            multitrack,
        });
        match started {
            Ok(r) => Some(r),
            Err(e) => {
                let _ = recs.store.delete_recording(&id);
                return Err(err(e));
            }
        }
    };

    // Subscribe before taking the snapshot so nothing falls between them.
    let mut changes = state.control.subscribe();
    let mut log = ControlLog::new(id.clone(), started_at);
    log.snapshot(&recs.store, &state.control.snapshot())
        .map_err(err)?;

    let store = recs.store.clone();
    let status = recs.status.clone();
    let folder = recs.folder();
    let (stop_tx, mut stop_rx) = oneshot::channel();
    let emitter = app.clone();
    let task_id = id.clone();
    let task = tauri::async_runtime::spawn(async move {
        let mut audio = audio;
        let mut warning: Option<String> = None;
        let mut auto_stopped = false;
        let mut ticker = tokio::time::interval(TICK);
        let final_status = loop {
            tokio::select! {
                _ = &mut stop_rx => break RecordingStatus::Complete,
                change = changes.recv() => match change {
                    Ok(change) => {
                        if let Err(e) = log.record(&store, change) {
                            log::error!("couldn't save a control change: {e}");
                        }
                    }
                    Err(RecvError::Lagged(n)) => {
                        log::warn!("recording missed {n} control changes");
                        warning = Some(format!("{n} control changes were missed while the computer was busy."));
                    }
                    Err(RecvError::Closed) => break RecordingStatus::Interrupted,
                },
                _ = ticker.tick() => {
                    let now = recorder::now_ms();
                    if let Err(e) = log.flush_if_due(&store, now) {
                        log::error!("couldn't save control changes: {e}");
                    }
                    let free = disk::free_bytes(&folder).unwrap_or(u64::MAX);
                    let multitrack_on = audio.as_ref().is_some_and(|a| a.stats().multitrack_active);
                    match disk::check_running(free, multitrack_on) {
                        DiskAction::Continue => {}
                        DiskAction::StopMultitrack => {
                            if let Some(a) = audio.as_ref() {
                                a.stop_multitrack();
                            }
                            warning = Some("The disk is nearly full, so multitrack stopped. The stereo mix and moves are still recording.".into());
                        }
                        DiskAction::StopAll => {
                            warning = Some("The disk is full, so recording stopped and was saved.".into());
                            auto_stopped = true;
                            break RecordingStatus::Complete;
                        }
                    }
                    let bytes = audio.as_ref().map(|a| a.stats().bytes_written).unwrap_or(0);
                    let snapshot = {
                        let mut s = status.lock().unwrap();
                        s.elapsed_ms = now.saturating_sub(started_at);
                        s.bytes_written = bytes;
                        s.free_bytes = free;
                        s.warning = warning.clone();
                        s.clone()
                    };
                    let _ = emitter.emit("recording", snapshot);
                }
            }
        };
        let saved = finish(&store, log, audio.take(), &task_id, final_status).await;
        if auto_stopped {
            // Nobody pressed Stop, so clear the slot and tell the UI ourselves.
            let state = emitter.state::<AppState>();
            if let Some(recs) = state.recordings.as_ref() {
                if let Ok(mut active) = recs.active.try_lock() {
                    if active.as_ref().is_some_and(|a| a.id == task_id) {
                        active.take();
                    }
                }
                let snapshot = {
                    let mut s = recs.status.lock().unwrap();
                    s.active = None;
                    s.warning = warning;
                    s.clone()
                };
                let _ = emitter.emit("recording", snapshot);
            }
        }
        saved
    });

    {
        let mut s = recs.status.lock().unwrap();
        *s = RecorderStatus {
            active: Some(summary_of(&recs.store, &id)?),
            elapsed_ms: 0,
            bytes_written: 0,
            free_bytes: free,
            warning: None,
        };
        let _ = app.emit("recording", s.clone());
    }
    *active = Some(Active {
        id: id.clone(),
        stop: stop_tx,
        task,
    });
    summary_of(&recs.store, &id)
}

/// Finalizes audio files, then the control log and bundle. Hashing gigabytes
/// of audio takes a few seconds, so it runs on a blocking thread.
async fn finish(
    store: &Arc<Store>,
    mut log: ControlLog,
    audio: Option<AudioRecorder>,
    id: &str,
    status: RecordingStatus,
) -> CmdResult<RecordingSummary> {
    let ended_at = recorder::now_ms();
    log.flush(store, ended_at).map_err(err)?;
    let store2 = store.clone();
    let id2 = id.to_owned();
    tauri::async_runtime::spawn_blocking(move || -> CmdResult<()> {
        if let Some(audio) = audio {
            for file in audio.stop().map_err(err)? {
                let kind = match file.kind {
                    AudioFileKind::Mix => FileKind::Mix,
                    AudioFileKind::Track => FileKind::Track,
                };
                let row = store2
                    .add_file(&id2, kind, file.channel, &file.rel_path)
                    .map_err(err)?;
                store2.finalize_file(&row).map_err(err)?;
            }
        }
        Ok(())
    })
    .await
    .map_err(err)??;
    log.finish(store, ended_at, status).map_err(err)?;
    summary_of(store, id)
}

fn summary_of(store: &Store, id: &str) -> CmdResult<RecordingSummary> {
    store.get_recording(id).map(|d| d.summary).map_err(err)
}

#[tauri::command]
pub async fn stop_recording(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<Option<RecordingSummary>> {
    let recs = recordings(&state)?;
    let Some(active) = recs.active.lock().await.take() else {
        return Ok(None);
    };
    let _ = active.stop.send(());
    let saved = active.task.await.map_err(err)??;
    log::info!("recording {} saved", active.id);
    let status = {
        let mut s = recs.status.lock().unwrap();
        s.active = None;
        s.clone()
    };
    let _ = app.emit("recording", status);
    // Today this is LocalOnly and does nothing; a cloud provider uploads here.
    if recs.sync.is_enabled() {
        if let Err(e) = recs.sync.push_pending(&recs.store).await {
            log::warn!("couldn't sync recordings, will retry later: {e}");
        }
    }
    Ok(Some(saved))
}

#[tauri::command]
pub async fn recorder_status(state: State<'_, AppState>) -> CmdResult<RecorderStatus> {
    let recs = recordings(&state)?;
    let mut status = recs.status.lock().unwrap().clone();
    if status.active.is_none() {
        status.free_bytes = disk::free_bytes(&recs.folder()).unwrap_or(0);
    }
    Ok(status)
}

#[tauri::command]
pub async fn list_recordings(state: State<'_, AppState>) -> CmdResult<Vec<RecordingSummary>> {
    recordings(&state)?.store.list_recordings().map_err(err)
}

#[tauri::command]
pub async fn get_recording(state: State<'_, AppState>, id: String) -> CmdResult<RecordingDetail> {
    recordings(&state)?.store.get_recording(&id).map_err(err)
}

#[tauri::command]
pub async fn recording_events(
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<Vec<RecordedEvent>> {
    let store = recordings(&state)?.store.clone();
    tauri::async_runtime::spawn_blocking(move || store.events(&id).map_err(err))
        .await
        .map_err(err)?
}

#[tauri::command]
pub async fn update_recording(
    state: State<'_, AppState>,
    id: String,
    title: String,
    notes: String,
) -> CmdResult<RecordingSummary> {
    let store = &recordings(&state)?.store;
    store
        .update_title_notes(&id, title.trim(), &notes)
        .map_err(err)?;
    summary_of(store, &id)
}

#[tauri::command]
pub async fn delete_recording(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let recs = recordings(&state)?;
    if recs
        .active
        .lock()
        .await
        .as_ref()
        .is_some_and(|a| a.id == id)
    {
        return Err("This service is still recording. Stop it before deleting.".into());
    }
    if state.auth.current_session().await.role == auth::Role::Volunteer {
        return Err("Ask an engineer or admin to delete recordings.".into());
    }
    let store = recs.store.clone();
    tauri::async_runtime::spawn_blocking(move || store.delete_recording(&id).map_err(err))
        .await
        .map_err(err)?
}

#[tauri::command]
pub async fn disk_usage(state: State<'_, AppState>) -> CmdResult<DiskUsage> {
    let recs = recordings(&state)?;
    let folder = recs.folder();
    Ok(DiskUsage {
        free_bytes: disk::free_bytes(&folder).map_err(err)?,
        recordings_bytes: recs.store.recordings_bytes().map_err(err)?,
        multitrack_bytes: recs.store.multitrack_bytes().map_err(err)?,
        folder: folder.display().to_string(),
    })
}

#[tauri::command]
pub async fn delete_old_multitracks(state: State<'_, AppState>, days: u32) -> CmdResult<u64> {
    if state.auth.current_session().await.role == auth::Role::Volunteer {
        return Err("Ask an engineer or admin to delete recordings.".into());
    }
    let store = recordings(&state)?.store.clone();
    tauri::async_runtime::spawn_blocking(move || {
        store
            .delete_multitracks_older_than(days, recorder::now_ms())
            .map_err(err)
    })
    .await
    .map_err(err)?
}
