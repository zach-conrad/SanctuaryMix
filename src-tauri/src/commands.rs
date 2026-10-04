use audio_engine::{AudioDeviceInfo, MicAccess};
use auth::Session;
use automix::{Adjustment, AutoMixConfig, AutoMixStatus, ChannelRole, Preset, RoomFeel};
use console::{ConsoleConfig, ConsoleError};
use mix_core::{ChangeSource, ChannelId, ChannelKind, ConsoleEvent};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::AppState;

/// Errors reach the UI as plain strings.
type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

// Audio commands are async and do their CoreAudio work on a blocking thread.
// Plain `fn` commands run on the main thread, and a device call that waits on
// the macOS microphone prompt there freezes the whole window.

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> CmdResult<T> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(err)?
}

#[tauri::command]
pub fn microphone_access() -> MicAccess {
    audio_engine::microphone_access()
}

/// Shows the macOS prompt if the user hasn't answered yet, once.
#[tauri::command]
pub async fn request_microphone_access() -> CmdResult<MicAccess> {
    blocking(|| Ok(audio_engine::request_microphone_access())).await
}

/// Opens System Settings at Privacy & Security › Microphone.
#[tauri::command]
pub fn open_microphone_settings() -> CmdResult<()> {
    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
        .spawn()
        .map_err(err)?;
    Ok(())
}

#[tauri::command]
pub async fn list_audio_devices() -> CmdResult<Vec<AudioDeviceInfo>> {
    blocking(|| audio_engine::list_input_devices().map_err(err)).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeteringInfo {
    device_name: String,
    channels: u16,
    sample_rate: u32,
}

#[tauri::command]
pub async fn start_metering(app: AppHandle, device: Option<String>) -> CmdResult<MeteringInfo> {
    blocking(move || {
        let state = app.state::<AppState>();
        let mut slot = state.metering.lock().unwrap();
        // Stop the old stream before opening the device again.
        slot.take();
        let emitter = app.clone();
        let automix = state.automix.clone();
        let handle = audio_engine::start_metering(device, move |frame| {
            automix.push_meters(frame.clone());
            let _ = emitter.emit("meters", frame);
        })
        .map_err(err)?;
        let info = MeteringInfo {
            device_name: handle.device_name.clone(),
            channels: handle.channels,
            sample_rate: handle.sample_rate,
        };
        *slot = Some(handle);
        Ok(info)
    })
    .await
}

#[tauri::command]
pub async fn stop_metering(app: AppHandle) -> CmdResult<()> {
    blocking(move || {
        app.state::<AppState>().metering.lock().unwrap().take();
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn connect_console(
    app: AppHandle,
    state: State<'_, AppState>,
    config: ConsoleConfig,
) -> CmdResult<()> {
    let input_count = config.input_count;
    let mut slot = state.console.lock().await;
    if let Some(mut old) = slot.take() {
        old.disconnect().await;
    }

    let mut adapter = console::create_adapter(config);
    let mut events = adapter.subscribe();
    let automix = state.automix.clone();
    let forwarder = tauri::async_runtime::spawn(async move {
        use tokio::sync::broadcast::error::RecvError;
        loop {
            match events.recv().await {
                Ok(event) => {
                    automix.push_console(event.clone());
                    let _ = app.emit("console", &event);
                    app.state::<AppState>()
                        .control
                        .publish(ChangeSource::Console, event);
                }
                Err(RecvError::Lagged(n)) => log::warn!("UI missed {n} console events"),
                Err(RecvError::Closed) => break,
            }
        }
    });
    if let Some(old) = state.console_forwarder.lock().unwrap().replace(forwarder) {
        old.abort();
    }

    adapter.connect().await.map_err(err)?;
    for index in 0..input_count {
        let id = ChannelId {
            kind: ChannelKind::Input,
            index,
        };
        adapter.request_name(id).await.map_err(err)?;
        // Auto-mix needs to know where each fader sits before it may move it.
        adapter.request_fader(id).await.map_err(err)?;
    }
    *slot = Some(adapter);
    Ok(())
}

#[tauri::command]
pub async fn disconnect_console(state: State<'_, AppState>) -> CmdResult<()> {
    if let Some(mut adapter) = state.console.lock().await.take() {
        adapter.disconnect().await;
    }
    Ok(())
}

#[tauri::command]
pub async fn set_fader(
    state: State<'_, AppState>,
    id: ChannelId,
    db: Option<f32>,
) -> CmdResult<()> {
    let slot = state.console.lock().await;
    let adapter = slot
        .as_ref()
        .ok_or(ConsoleError::NotConnected)
        .map_err(err)?;
    adapter.set_fader(id, db).await.map_err(err)?;
    state
        .control
        .publish(ChangeSource::Operator, ConsoleEvent::Fader { id, db });
    Ok(())
}

#[tauri::command]
pub async fn set_mute(state: State<'_, AppState>, id: ChannelId, muted: bool) -> CmdResult<()> {
    let slot = state.console.lock().await;
    let adapter = slot
        .as_ref()
        .ok_or(ConsoleError::NotConnected)
        .map_err(err)?;
    adapter.set_mute(id, muted).await.map_err(err)?;
    state
        .control
        .publish(ChangeSource::Operator, ConsoleEvent::Mute { id, muted });
    Ok(())
}

#[tauri::command]
pub async fn request_channel_names(
    state: State<'_, AppState>,
    ids: Vec<ChannelId>,
) -> CmdResult<()> {
    let slot = state.console.lock().await;
    let adapter = slot
        .as_ref()
        .ok_or(ConsoleError::NotConnected)
        .map_err(err)?;
    for id in ids {
        adapter.request_name(id).await.map_err(err)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_session(state: State<'_, AppState>) -> CmdResult<Session> {
    Ok(state.auth.current_session().await)
}

#[tauri::command]
pub async fn begin_sign_in(state: State<'_, AppState>) -> CmdResult<String> {
    state.auth.begin_sign_in().await.map_err(err)
}

#[tauri::command]
pub async fn complete_sign_in(
    state: State<'_, AppState>,
    callback_url: String,
) -> CmdResult<Session> {
    state.auth.complete_sign_in(callback_url).await.map_err(err)
}

#[tauri::command]
pub async fn sign_out(state: State<'_, AppState>) -> CmdResult<Session> {
    Ok(state.auth.sign_out().await)
}

// Auto-mix. The rules live in the automix crate; these only pass requests
// through, so nothing the UI sends can loosen a guardrail.

#[tauri::command]
pub async fn automix_presets() -> Vec<Preset> {
    RoomFeel::ALL.iter().map(|f| f.preset()).collect()
}

#[tauri::command]
pub async fn automix_guess_roles(names: Vec<String>) -> Vec<ChannelRole> {
    names.iter().map(|n| automix::guess_role(n)).collect()
}

#[tauri::command]
pub async fn automix_get_config(app: AppHandle) -> CmdResult<AutoMixConfig> {
    blocking(move || {
        let state = app.state::<AppState>();
        let saved = state
            .store
            .lock()
            .unwrap()
            .get_setting::<AutoMixConfig>(store::keys::AUTOMIX_CONFIG)
            .map_err(err)?;
        Ok(saved.unwrap_or_default().sanitized())
    })
    .await
}

/// Applies and saves the operator's choices. Returns them as the core will use them.
#[tauri::command]
pub async fn automix_set_config(app: AppHandle, config: AutoMixConfig) -> CmdResult<AutoMixConfig> {
    let preset = config.feel.preset();
    if preset.admin_only {
        let session = app.state::<AppState>().auth.current_session().await;
        if session.role != auth::Role::Admin {
            return Err(format!(
                "Only an admin can choose {}. Ask an admin, or pick another room feel.",
                preset.name.to_lowercase()
            ));
        }
    }
    let applied = app
        .state::<AppState>()
        .automix
        .configure(config)
        .await
        .map_err(err)?;
    let saved = applied.clone();
    blocking(move || {
        app.state::<AppState>()
            .store
            .lock()
            .unwrap()
            .set_setting(store::keys::AUTOMIX_CONFIG, &saved)
            .map_err(err)
    })
    .await?;
    Ok(applied)
}

#[tauri::command]
pub async fn automix_engage(state: State<'_, AppState>, on: bool) -> CmdResult<()> {
    state.automix.engage(on).await.map_err(err)
}

/// Stops every automatic move immediately.
#[tauri::command]
pub async fn automix_freeze(state: State<'_, AppState>) -> CmdResult<()> {
    state.automix.freeze();
    Ok(())
}

#[tauri::command]
pub async fn automix_resume(state: State<'_, AppState>) -> CmdResult<()> {
    state.automix.unfreeze().await.map_err(err)
}

#[tauri::command]
pub async fn automix_resume_channel(state: State<'_, AppState>, channel: u16) -> CmdResult<()> {
    state.automix.resume_channel(channel).await.map_err(err)
}

#[tauri::command]
pub async fn automix_undo(state: State<'_, AppState>, channel: u16) -> CmdResult<()> {
    state.automix.undo(channel).await.map_err(err)
}

#[tauri::command]
pub async fn automix_undo_all(state: State<'_, AppState>) -> CmdResult<()> {
    state.automix.undo_all().await.map_err(err)
}

#[tauri::command]
pub async fn automix_status(state: State<'_, AppState>) -> CmdResult<AutoMixStatus> {
    state.automix.status().await.map_err(err)
}

/// The most recent auto-mix log entries, newest first.
#[tauri::command]
pub async fn automix_log(app: AppHandle, limit: Option<u32>) -> CmdResult<Vec<Adjustment>> {
    blocking(move || {
        app.state::<AppState>()
            .store
            .lock()
            .unwrap()
            .recent_adjustments(limit.unwrap_or(200).min(5_000))
            .map_err(err)
    })
    .await
}
