//! Listening to a recorded service, and the guarded replay of its moves.
//!
//! One transport drives both: the audio player when the recording has a mix,
//! or a plain clock when it only has moves. Replay follows that transport and
//! sends each recorded fader and mute move to the console as the playhead
//! passes it. See docs/RECORDINGS.md for the safeguards.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use audio_engine::{OutputDeviceInfo, Player};
use mix_core::{ChangeSource, ChannelId, ChannelState, ConsoleEvent};
use recorder::{replay, AudioMode, FileKind, RecordedEvent};
use serde::Serialize;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::broadcast::error::RecvError;

use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

const STATUS_INTERVAL: Duration = Duration::from_millis(100);
const REPLAY_INTERVAL: Duration = Duration::from_millis(40);
/// A console report this close to something replay sent is its echo.
const ECHO_WINDOW: Duration = Duration::from_millis(600);
const ECHO_TOLERANCE_DB: f32 = 0.5;
/// Jumps longer than this (a seek) send the state at the new spot, not every move in between.
const JUMP_MS: u64 = 2_000;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackStatus {
    recording_id: Option<String>,
    position_ms: u64,
    duration_ms: u64,
    playing: bool,
    device: Option<String>,
    has_audio: bool,
}

/// Position for recordings without audio.
#[derive(Default)]
struct Clock {
    base_ms: u64,
    since: Option<Instant>,
}

impl Clock {
    fn position(&self, duration: u64) -> u64 {
        let run = self.since.map_or(0, |s| s.elapsed().as_millis() as u64);
        (self.base_ms + run).min(duration)
    }
}

struct Loaded {
    recording_id: String,
    duration_ms: u64,
    player: Option<Player>,
    clock: Clock,
}

impl Loaded {
    fn position(&self) -> u64 {
        match &self.player {
            Some(p) => p.position_ms(),
            None => self.clock.position(self.duration_ms),
        }
    }

    fn playing(&mut self) -> bool {
        match &self.player {
            Some(p) => p.is_playing(),
            None => {
                // The clock stops itself at the end.
                if self.clock.since.is_some()
                    && self.clock.position(self.duration_ms) >= self.duration_ms
                {
                    self.clock = Clock {
                        base_ms: self.duration_ms,
                        since: None,
                    };
                }
                self.clock.since.is_some()
            }
        }
    }

    fn play(&mut self) {
        match &self.player {
            Some(p) => p.play(),
            None if self.clock.since.is_none() => {
                if self.clock.base_ms >= self.duration_ms {
                    self.clock.base_ms = 0;
                }
                self.clock.since = Some(Instant::now());
            }
            None => {}
        }
    }

    fn pause(&mut self) {
        match &self.player {
            Some(p) => p.pause(),
            None => {
                self.clock = Clock {
                    base_ms: self.clock.position(self.duration_ms),
                    since: None,
                }
            }
        }
    }

    fn seek(&mut self, ms: u64) {
        let ms = ms.min(self.duration_ms);
        match &self.player {
            Some(p) => p.seek(ms),
            None => {
                self.clock.base_ms = ms;
                if self.clock.since.is_some() {
                    self.clock.since = Some(Instant::now());
                }
            }
        }
    }

    fn status(&mut self) -> PlaybackStatus {
        PlaybackStatus {
            recording_id: Some(self.recording_id.clone()),
            position_ms: self.position(),
            duration_ms: self.duration_ms,
            playing: self.playing(),
            device: self.player.as_ref().map(|p| p.device_name.clone()),
            has_audio: self.player.is_some(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ReplayState {
    Idle,
    Running,
    Stopped,
    TakenOver,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayStatus {
    state: ReplayState,
    recording_id: Option<String>,
    channels: Option<Vec<ChannelId>>,
    sent_count: u64,
    can_restore: bool,
    message: Option<String>,
}

impl Default for ReplayStatus {
    fn default() -> Self {
        Self {
            state: ReplayState::Idle,
            recording_id: None,
            channels: None,
            sent_count: 0,
            can_restore: false,
            message: None,
        }
    }
}

#[derive(Default)]
pub struct Playback {
    loaded: Arc<Mutex<Option<Loaded>>>,
    ticker: Mutex<Option<JoinHandle<()>>>,
    replay: Arc<Mutex<ReplayStatus>>,
    replay_task: Mutex<Option<JoinHandle<()>>>,
    /// Console state captured before the last replay, for Restore.
    before_replay: Mutex<Vec<ChannelState>>,
}

impl Playback {
    fn status(&self) -> PlaybackStatus {
        self.loaded
            .lock()
            .unwrap()
            .as_mut()
            .map(Loaded::status)
            .unwrap_or_default()
    }

    fn position(&self) -> Option<(String, u64, bool)> {
        self.loaded
            .lock()
            .unwrap()
            .as_mut()
            .map(|l| (l.recording_id.clone(), l.position(), l.playing()))
    }

    fn set_replay(&self, app: &AppHandle, update: impl FnOnce(&mut ReplayStatus)) {
        let status = {
            let mut s = self.replay.lock().unwrap();
            update(&mut s);
            s.clone()
        };
        let _ = app.emit("replay", status);
    }

    /// Ends a running replay, if any.
    fn halt_replay(&self) {
        if let Some(task) = self.replay_task.lock().unwrap().take() {
            task.abort();
        }
    }
}

#[tauri::command]
pub async fn list_output_devices() -> CmdResult<Vec<OutputDeviceInfo>> {
    tauri::async_runtime::spawn_blocking(|| audio_engine::list_output_devices().map_err(err))
        .await
        .map_err(err)?
}

#[tauri::command]
pub async fn load_playback(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    device: Option<String>,
) -> CmdResult<PlaybackStatus> {
    let recs = state
        .recordings
        .as_ref()
        .ok_or("Recordings aren't available on this computer.")?;
    let detail = recs.store.get_recording(&id).map_err(err)?;
    let mix = detail
        .files
        .iter()
        .find(|f| f.kind == FileKind::Mix)
        .map(|f| recs.store.recording_dir(&id).join(&f.rel_path));
    let has_mix =
        detail.summary.audio_mode != AudioMode::None && mix.as_ref().is_some_and(|p| p.exists());

    // Reloading the same service (a new output device) keeps the playhead.
    let resume = state
        .playback
        .position()
        .filter(|(loaded, _, _)| *loaded == id)
        .map(|(_, pos, playing)| (pos, playing));
    state.playback.halt_replay();
    if matches!(
        state.playback.replay.lock().unwrap().state,
        ReplayState::Running
    ) {
        state.playback.set_replay(&app, |s| {
            s.state = ReplayState::Stopped;
            s.message = Some(
                "Replay stopped because playback changed. Press Send moves to continue.".into(),
            );
        });
    }
    // Close the old player before opening the device again.
    state.playback.loaded.lock().unwrap().take();

    let player = match (has_mix, mix) {
        (true, Some(path)) => Some(
            tauri::async_runtime::spawn_blocking(move || Player::open(&path, device, |_| {}))
                .await
                .map_err(err)?
                .map_err(err)?,
        ),
        _ => None,
    };
    let duration_ms = player
        .as_ref()
        .map(Player::duration_ms)
        .unwrap_or(0)
        .max(detail.summary.duration_ms);
    let mut loaded = Loaded {
        recording_id: id,
        duration_ms,
        player,
        clock: Clock::default(),
    };
    if let Some((pos, playing)) = resume {
        loaded.seek(pos);
        if playing {
            loaded.play();
        }
    }
    *state.playback.loaded.lock().unwrap() = Some(loaded);

    let mut ticker = state.playback.ticker.lock().unwrap();
    if ticker.is_none() {
        let app = app.clone();
        *ticker = Some(tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(STATUS_INTERVAL);
            loop {
                interval.tick().await;
                let status = app.state::<AppState>().playback.status();
                if status.recording_id.is_some() {
                    let _ = app.emit("playback", status);
                }
            }
        }));
    }
    Ok(state.playback.status())
}

fn with_loaded(state: &AppState, f: impl FnOnce(&mut Loaded)) -> CmdResult<()> {
    let mut loaded = state.playback.loaded.lock().unwrap();
    f(loaded.as_mut().ok_or("Open a service first.")?);
    Ok(())
}

#[tauri::command]
pub async fn play_recording(state: State<'_, AppState>) -> CmdResult<()> {
    with_loaded(&state, Loaded::play)
}

#[tauri::command]
pub async fn pause_recording(state: State<'_, AppState>) -> CmdResult<()> {
    with_loaded(&state, Loaded::pause)
}

#[tauri::command]
pub async fn seek_recording(state: State<'_, AppState>, position_ms: u64) -> CmdResult<()> {
    with_loaded(&state, |l| l.seek(position_ms))
}

#[tauri::command]
pub async fn unload_playback(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    state.playback.halt_replay();
    state.playback.loaded.lock().unwrap().take();
    let _ = app.emit("playback", PlaybackStatus::default());
    if matches!(
        state.playback.replay.lock().unwrap().state,
        ReplayState::Running
    ) {
        state.playback.set_replay(&app, |s| {
            s.state = ReplayState::Stopped;
            s.message = Some("Replay stopped because the service was closed.".into());
        });
    }
    Ok(())
}

/// Sends `event` to the console and tags it as a replay move on the bus.
async fn send(state: &AppState, event: &ConsoleEvent) -> CmdResult<()> {
    let slot = state.console.lock().await;
    let adapter = slot
        .as_ref()
        .ok_or("The console disconnected, so replay stopped.")?;
    match *event {
        ConsoleEvent::Fader { id, db } => adapter.set_fader(id, db).await.map_err(err)?,
        ConsoleEvent::Mute { id, muted } => adapter.set_mute(id, muted).await.map_err(err)?,
        _ => return Ok(()),
    }
    state.control.publish(ChangeSource::Replay, event.clone());
    Ok(())
}

/// The fader and mute moves that bring the console to `states`.
fn state_events(states: &[ChannelState], channels: Option<&[ChannelId]>) -> Vec<ConsoleEvent> {
    states
        .iter()
        .filter(|s| channels.is_none_or(|c| c.contains(&s.id)))
        .flat_map(|s| {
            [
                ConsoleEvent::Fader {
                    id: s.id,
                    db: s.fader_db,
                },
                ConsoleEvent::Mute {
                    id: s.id,
                    muted: s.muted,
                },
            ]
        })
        .collect()
}

/// True when a console report matches something replay just sent.
fn is_echo(
    sent: &HashMap<(ChannelId, bool), (ConsoleEvent, Instant)>,
    event: &ConsoleEvent,
) -> bool {
    let key = match event {
        ConsoleEvent::Fader { id, .. } => (*id, true),
        ConsoleEvent::Mute { id, .. } => (*id, false),
        // Names and connection changes aren't anyone taking over.
        _ => return true,
    };
    let Some((sent_event, at)) = sent.get(&key) else {
        return false;
    };
    if at.elapsed() > ECHO_WINDOW {
        return false;
    }
    match (sent_event, event) {
        (ConsoleEvent::Fader { db: a, .. }, ConsoleEvent::Fader { db: b, .. }) => match (a, b) {
            (Some(a), Some(b)) => (a - b).abs() <= ECHO_TOLERANCE_DB,
            (a, b) => a.is_none() && b.is_none_or(|b| b < -90.0),
        },
        (ConsoleEvent::Mute { muted: a, .. }, ConsoleEvent::Mute { muted: b, .. }) => a == b,
        _ => false,
    }
}

#[tauri::command]
pub async fn start_replay(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    from_ms: u64,
    channels: Option<Vec<ChannelId>>,
) -> CmdResult<ReplayStatus> {
    let session = state.auth.current_session().await;
    if session.role == auth::Role::Volunteer {
        return Err("Ask an engineer or admin to send recorded moves to the console.".into());
    }
    let recs = state
        .recordings
        .as_ref()
        .ok_or("Recordings aren't available on this computer.")?;
    if recs.is_recording().await {
        return Err(
            "A service is recording. Stop recording before sending moves to the console.".into(),
        );
    }
    if !state
        .console
        .lock()
        .await
        .as_ref()
        .is_some_and(|c| c.is_connected())
    {
        return Err("Connect to the console in Setup first.".into());
    }
    let store = recs.store.clone();
    let rec_id = id.clone();
    let events: Vec<RecordedEvent> =
        tauri::async_runtime::spawn_blocking(move || store.events(&rec_id))
            .await
            .map_err(err)?
            .map_err(err)?;

    state.playback.halt_replay();
    // Auto-mix would read replayed moves as a person taking over its faders.
    state
        .automix
        .engage(false)
        .await
        .map_err(|_| "Auto-mix isn't responding, so replay didn't start.".to_string())?;

    // Remember the console as it is now, for Restore.
    let before: Vec<ChannelState> = state
        .control
        .snapshot()
        .into_iter()
        .filter(|s| channels.as_ref().is_none_or(|c| c.contains(&s.id)))
        .collect();
    *state.playback.before_replay.lock().unwrap() = before;

    // Follow this recording's transport, starting where the user asked.
    let loaded_here = state
        .playback
        .position()
        .is_some_and(|(loaded, _, _)| loaded == id);
    if !loaded_here {
        load_playback(app.clone(), state.clone(), id.clone(), None).await?;
    }
    with_loaded(&state, |l| {
        l.seek(from_ms);
        l.play();
    })?;

    // Bring the console to the recorded state at the start point.
    let start_state = replay::state_at(&events, from_ms);
    let mut sent: HashMap<(ChannelId, bool), (ConsoleEvent, Instant)> = HashMap::new();
    let mut sent_count = 0;
    for event in state_events(&start_state, channels.as_deref()) {
        send(&state, &event).await?;
        sent.insert(key_of(&event), (event, Instant::now()));
        sent_count += 1;
    }

    state.playback.set_replay(&app, |s| {
        *s = ReplayStatus {
            state: ReplayState::Running,
            recording_id: Some(id.clone()),
            channels: channels.clone(),
            sent_count,
            can_restore: true,
            message: None,
        }
    });

    let mut changes = state.control.subscribe();
    let task_app = app.clone();
    let task = tauri::async_runtime::spawn(async move {
        let state = task_app.state::<AppState>();
        let mut last = from_ms;
        let mut paused_ticks = 0u32;
        let mut interval = tokio::time::interval(REPLAY_INTERVAL);
        let channels = channels.as_deref();
        let end = |state: &AppState, kind: ReplayState, message: &str| {
            state.playback.set_replay(&task_app, |s| {
                s.state = kind;
                s.message = Some(message.into());
            });
        };
        loop {
            tokio::select! {
                change = changes.recv() => match change {
                    Ok(c) => {
                        let takeover = match c.source {
                            ChangeSource::Operator | ChangeSource::Assist => true,
                            ChangeSource::Console => !is_echo(&sent, &c.event),
                            ChangeSource::Replay | ChangeSource::Snapshot => false,
                        };
                        if takeover {
                            with_loaded(&state, Loaded::pause).ok();
                            return end(&state, ReplayState::TakenOver, "Someone moved a control, so replay stopped and handed the console back.");
                        }
                    }
                    Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => return,
                },
                _ = interval.tick() => {
                    let Some((loaded, pos, playing)) = state.playback.position() else {
                        return end(&state, ReplayState::Stopped, "Replay stopped because the service was closed.");
                    };
                    if loaded != id {
                        return end(&state, ReplayState::Stopped, "Replay stopped because another service was opened.");
                    }
                    let batch: Vec<ConsoleEvent> = if pos < last || pos - last > JUMP_MS {
                        // A seek: jump straight to the recorded state there.
                        state_events(&replay::state_at(&events, pos), channels)
                    } else {
                        replay::moves_between(&events, last, pos, channels)
                            .map(|e| e.event.clone())
                            .collect()
                    };
                    last = pos;
                    for event in batch {
                        if let Err(e) = send(&state, &event).await {
                            return end(&state, ReplayState::Stopped, &e);
                        }
                        sent.insert(key_of(&event), (event, Instant::now()));
                        state.playback.replay.lock().unwrap().sent_count += 1;
                    }
                    paused_ticks = if playing { 0 } else { paused_ticks + 1 };
                    // Give the player a moment to start before reading a pause as the end.
                    if paused_ticks > 15 {
                        return end(&state, ReplayState::Stopped, "Replay paused with the service. Press Send moves again to continue.");
                    }
                }
            }
        }
    });
    *state.playback.replay_task.lock().unwrap() = Some(task);
    Ok(state.playback.replay.lock().unwrap().clone())
}

fn key_of(event: &ConsoleEvent) -> (ChannelId, bool) {
    match event {
        ConsoleEvent::Fader { id, .. } => (*id, true),
        ConsoleEvent::Mute { id, .. } => (*id, false),
        _ => (ChannelId::input(u16::MAX), false),
    }
}

#[tauri::command]
pub async fn stop_replay(app: AppHandle, state: State<'_, AppState>) -> CmdResult<ReplayStatus> {
    state.playback.halt_replay();
    state.playback.set_replay(&app, |s| {
        if matches!(s.state, ReplayState::Running) {
            s.state = ReplayState::Stopped;
            s.message = None;
        }
    });
    Ok(state.playback.replay.lock().unwrap().clone())
}

#[tauri::command]
pub async fn restore_before_replay(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<ReplayStatus> {
    state.playback.halt_replay();
    let before = std::mem::take(&mut *state.playback.before_replay.lock().unwrap());
    if before.is_empty() {
        return Err("There's nothing to restore.".into());
    }
    for event in state_events(&before, None) {
        send(&state, &event).await?;
    }
    state.playback.set_replay(&app, |s| {
        s.state = ReplayState::Idle;
        s.can_restore = false;
        s.message = Some("The console is back to how it was before replay.".into());
    });
    Ok(state.playback.replay.lock().unwrap().clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn echoes_of_replay_sends_are_not_takeovers() {
        let ch = ChannelId::input(1);
        let mut sent = HashMap::new();
        let ev = ConsoleEvent::Fader {
            id: ch,
            db: Some(-4.0),
        };
        sent.insert(key_of(&ev), (ev, Instant::now()));
        assert!(is_echo(
            &sent,
            &ConsoleEvent::Fader {
                id: ch,
                db: Some(-4.2)
            }
        ));
        assert!(!is_echo(
            &sent,
            &ConsoleEvent::Fader {
                id: ch,
                db: Some(-1.0)
            }
        ));
        assert!(!is_echo(
            &sent,
            &ConsoleEvent::Mute {
                id: ch,
                muted: true
            }
        ));
        assert!(is_echo(
            &sent,
            &ConsoleEvent::Name {
                id: ch,
                name: "Kick".into()
            }
        ));
    }

    #[test]
    fn clock_runs_and_stops_at_the_end() {
        let mut l = Loaded {
            recording_id: "r".into(),
            duration_ms: 1_000,
            player: None,
            clock: Clock::default(),
        };
        l.seek(400);
        assert_eq!(l.position(), 400);
        assert!(!l.playing());
        l.play();
        assert!(l.playing());
        l.seek(5_000);
        assert_eq!(l.position(), 1_000);
        assert!(!l.playing());
    }
}
