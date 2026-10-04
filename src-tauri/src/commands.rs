use audio_engine::AudioDeviceInfo;
use auth::{Credentials, Session};
use console::{ConsoleConfig, ConsoleError};
use mix_core::{ChannelId, ChannelKind};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::state::AppState;

/// Errors reach the UI as plain strings.
type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[tauri::command]
pub fn list_audio_devices() -> CmdResult<Vec<AudioDeviceInfo>> {
    audio_engine::list_input_devices().map_err(err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeteringInfo {
    device_name: String,
    channels: u16,
    sample_rate: u32,
}

#[tauri::command]
pub fn start_metering(
    app: AppHandle,
    state: State<'_, AppState>,
    device: Option<String>,
) -> CmdResult<MeteringInfo> {
    let mut slot = state.metering.lock().unwrap();
    // Stop the old stream before opening the device again.
    slot.take();
    let handle = audio_engine::start_metering(device, move |frame| {
        let _ = app.emit("meters", frame);
    })
    .map_err(err)?;
    let info = MeteringInfo {
        device_name: handle.device_name.clone(),
        channels: handle.channels,
        sample_rate: handle.sample_rate,
    };
    *slot = Some(handle);
    Ok(info)
}

#[tauri::command]
pub fn stop_metering(state: State<'_, AppState>) {
    state.metering.lock().unwrap().take();
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
    let forwarder = tauri::async_runtime::spawn(async move {
        use tokio::sync::broadcast::error::RecvError;
        loop {
            match events.recv().await {
                Ok(event) => {
                    let _ = app.emit("console", event);
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
        adapter
            .request_name(ChannelId {
                kind: ChannelKind::Input,
                index,
            })
            .await
            .map_err(err)?;
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
    adapter.set_fader(id, db).await.map_err(err)
}

#[tauri::command]
pub async fn set_mute(state: State<'_, AppState>, id: ChannelId, muted: bool) -> CmdResult<()> {
    let slot = state.console.lock().await;
    let adapter = slot
        .as_ref()
        .ok_or(ConsoleError::NotConnected)
        .map_err(err)?;
    adapter.set_mute(id, muted).await.map_err(err)
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
pub async fn sign_in(state: State<'_, AppState>, credentials: Credentials) -> CmdResult<Session> {
    state.auth.sign_in(credentials).await.map_err(err)
}

#[tauri::command]
pub async fn sign_out(state: State<'_, AppState>) -> CmdResult<Session> {
    Ok(state.auth.sign_out().await)
}
