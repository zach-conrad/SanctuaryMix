use audio_engine::{AudioDeviceInfo, MicAccess};
use auth::Session;
use automix::{Adjustment, AutoMixConfig, AutoMixStatus, ChannelRole, Preset, RoomFeel};
use console::{ConsoleConfig, ConsoleError};
use mix_core::hearing::Sound;
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
        let listener = state.listener.clone();
        let audio_app = app.clone();
        let handle = audio_engine::start_metering(
            device,
            move |frame| {
                automix.push_meters(frame.clone());
                let _ = emitter.emit("meters", frame);
            },
            move |block| {
                // A few biquads on one channel: cheap enough to run right here.
                audio_app.state::<AppState>().spl.push_audio(
                    &audio_app,
                    block.channels,
                    block.sample_rate,
                    block.samples,
                );
                if let Some(listener) = &listener {
                    listener.push_audio(block.channels, block.sample_rate, block.samples);
                }
            },
        )
        .map_err(err)?;
        state
            .spl
            .set_device(&app, &handle.device_name, handle.sample_rate);
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

/// Signs in from the app's email and password form. The password goes no
/// further than the auth provider.
#[tauri::command]
pub async fn sign_in_with_password(
    app: AppHandle,
    email: String,
    password: String,
) -> CmdResult<Session> {
    let session = app
        .state::<AppState>()
        .auth
        .sign_in_with_password(&email, &password)
        .await
        .map_err(err)?;
    remember_sign_in(app, true).await;
    Ok(session)
}

/// Billing lives on the website; card entry never happens in the app.
const BILLING_URL: &str = "https://sanctuarymix.vercel.app/account/";

/// Opens the website account page in the browser. Takes no URL so the UI
/// can't open anything else.
#[tauri::command]
pub fn open_billing() -> CmdResult<()> {
    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg(BILLING_URL)
        .spawn()
        .map_err(err)?;
    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer")
        .arg(BILLING_URL)
        .spawn()
        .map_err(err)?;
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    log::info!("open {BILLING_URL} in a browser");
    Ok(())
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

/// Signing out drops to the local session, which has no plan. Like any plan
/// change, that never happens mid-service.
#[tauri::command]
pub async fn sign_out(app: AppHandle) -> CmdResult<Session> {
    let state = app.state::<AppState>();
    if let Some(recs) = &state.recordings {
        if recs.is_recording().await {
            return Err("Stop recording the service before you sign out.".into());
        }
    }
    if state.automix.status().await.map_err(err)?.engaged {
        return Err("Turn off auto-mix before you sign out.".into());
    }
    let session = state.auth.sign_out().await;
    remember_sign_in(app, false).await;
    Ok(session)
}

/// Keeps the sample account signed in across launches. Real accounts will keep
/// their tokens in the keychain instead.
async fn remember_sign_in(app: AppHandle, signed_in: bool) {
    let saved = blocking(move || {
        app.state::<AppState>()
            .store
            .lock()
            .unwrap()
            .set_setting(store::keys::SIGNED_IN, &signed_in)
            .map_err(err)
    })
    .await;
    if let Err(e) = saved {
        log::warn!("couldn't save sign-in, it will be asked again next launch: {e}");
    }
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
    if let Some(listener) = &app.state::<AppState>().listener {
        listener.set_targets(crate::state::listen_targets(&applied));
    }
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

/// Turning on checks the plan; turning off never does.
#[tauri::command]
pub async fn automix_engage(app: AppHandle, on: bool) -> CmdResult<()> {
    let state = app.state::<AppState>();
    if on {
        let access = state.auth.current_session().await.access;
        let config = automix_get_config(app.clone()).await?;
        access.require_ai_channels(config.channels.len())?;
    }
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

/// Listens to every input with signal for `seconds`, so roles can be suggested by ear.
#[tauri::command]
pub async fn automix_listen_scan(state: State<'_, AppState>, seconds: u32) -> CmdResult<()> {
    let listener = state.listener.as_ref().ok_or(LISTENING_UNAVAILABLE)?;
    listener.scan(std::time::Duration::from_secs(seconds.clamp(5, 120) as u64));
    Ok(())
}

const LISTENING_UNAVAILABLE: &str =
    "Listening isn't available on this computer, so roles can only be guessed from channel names.";

/// What one input has mostly sounded like, and the role that would fit it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeardChannel {
    channel: u16,
    sound: Sound,
    share: f32,
    seconds: f32,
    /// Set when what it hears doesn't fit the role it has (or would get from its name).
    suggested_role: Option<ChannelRole>,
}

/// What each input has sounded like since the last scan or since auto-mix started listening.
#[tauri::command]
pub async fn automix_heard(
    state: State<'_, AppState>,
    names: Vec<String>,
) -> CmdResult<Vec<HeardChannel>> {
    let listener = state.listener.as_ref().ok_or(LISTENING_UNAVAILABLE)?;
    let managed = state.automix.status().await.map_err(err)?.channels;
    Ok(listener
        .heard()
        .into_iter()
        .map(|h| {
            let current = managed
                .iter()
                .find(|c| c.channel == h.channel)
                .map(|c| c.role)
                .unwrap_or_else(|| {
                    automix::guess_role(names.get(h.channel as usize).map_or("", |n| n.as_str()))
                });
            HeardChannel {
                channel: h.channel,
                sound: h.sound,
                share: h.share,
                seconds: h.seconds,
                suggested_role: automix::suggest_role(current, h.sound),
            }
        })
        .collect())
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
