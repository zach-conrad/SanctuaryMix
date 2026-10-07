//! Tauri shell: wires the Rust crates to the React UI.
//!
//! The UI calls the `#[tauri::command]`s in [`commands`] and listens for these
//! events: `meters` (a [`mix_core::MeterFrame`] ~30x a second), `console`
//! (a [`mix_core::ConsoleEvent`] whenever the desk changes), `automix` (an
//! [`automix::AutoMixStatus`] a few times a second) and `automix-adjustment`
//! (an [`automix::Adjustment`] for every auto-mix move or takeover),
//! `recording` (recorder status about once a second), `playback` (the
//! transport about 10x a second), `replay` (sending recorded moves), and
//! `session` (an [`auth::Session`] whenever sign-in changes outside a command:
//! website sign-in finishing, or the plan check between services) or
//! `session-error` (a sentence, when website sign-in fails), `aieq` (a
//! [`tonal::AiEqStatus`] a few times a second) and `aieq-log` (a
//! [`tonal::EqLogEntry`] for every EQ change, with who made it).

mod aieq;
mod commands;
mod control;
mod playback;
mod recording;
mod spl;
mod state;

use std::time::Duration;

use state::AppState;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_deep_link::DeepLinkExt;

/// How often a running app re-checks the account and plan, between services.
const PLAN_CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// While a service is on, how soon to look again.
const PLAN_CHECK_RETRY: Duration = Duration::from_secs(10 * 60);

pub fn run() {
    let builder = tauri::Builder::default();
    // Must come first: a second launch from a sign-in link hands the link to
    // this copy, which the deep-link plugin then delivers.
    #[cfg(any(windows, target_os = "linux"))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|_app, _argv, _cwd| {}));
    builder
        .plugin(tauri_plugin_deep_link::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .setup(|app| {
            let state = AppState::new(app.handle());
            app.manage(state);

            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    if url.scheme() == "sanctuarymix" {
                        tauri::async_runtime::spawn(finish_sign_in(
                            handle.clone(),
                            url.to_string(),
                        ));
                    }
                }
            });
            // Windows and Linux need the scheme registered at run time in dev builds.
            #[cfg(any(windows, target_os = "linux"))]
            if let Err(e) = app.deep_link().register_all() {
                log::warn!("couldn't register sanctuarymix:// links: {e}");
            }

            tauri::async_runtime::spawn(check_plan(app.handle().clone()));
            tauri::async_runtime::spawn(aieq::watch_service(app.handle().clone()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::microphone_access,
            commands::request_microphone_access,
            commands::open_microphone_settings,
            commands::list_audio_devices,
            commands::start_metering,
            commands::stop_metering,
            commands::connect_console,
            commands::disconnect_console,
            commands::set_fader,
            commands::set_mute,
            commands::request_channel_names,
            commands::automix_presets,
            commands::automix_guess_roles,
            commands::automix_get_config,
            commands::automix_set_config,
            commands::automix_engage,
            commands::automix_freeze,
            commands::automix_resume,
            commands::automix_resume_channel,
            commands::automix_undo,
            commands::automix_undo_all,
            commands::automix_status,
            commands::automix_log,
            commands::automix_listen_scan,
            commands::automix_heard,
            aieq::aieq_get_config,
            aieq::aieq_set_config,
            aieq::aieq_status,
            aieq::aieq_log,
            aieq::aieq_soundcheck_start,
            aieq::aieq_soundcheck_stop,
            aieq::aieq_apply,
            aieq::aieq_apply_all,
            aieq::aieq_skip,
            aieq::aieq_keep_my_eq,
            aieq::aieq_undo,
            aieq::aieq_undo_all,
            aieq::aieq_hand_back,
            aieq::aieq_set_eq,
            aieq::aieq_compare,
            aieq::aieq_restore_profile,
            aieq::aieq_dismiss_profile,
            aieq::aieq_ring_out_start,
            aieq::aieq_ring_out_stop,
            aieq::aieq_dismiss_feedback,
            aieq::aieq_ideas,
            aieq::aieq_set_idea,
            aieq::aieq_audit,
            recording::get_recording_settings,
            recording::set_recording_settings,
            recording::start_recording,
            recording::stop_recording,
            recording::recorder_status,
            recording::list_recordings,
            recording::get_recording,
            recording::recording_events,
            recording::update_recording,
            recording::delete_recording,
            recording::disk_usage,
            recording::delete_old_multitracks,
            playback::list_output_devices,
            playback::load_playback,
            playback::play_recording,
            playback::pause_recording,
            playback::seek_recording,
            playback::unload_playback,
            playback::start_replay,
            playback::stop_replay,
            playback::restore_before_replay,
            spl::spl_get_config,
            spl::spl_set_config,
            spl::spl_calibrate,
            spl::spl_cancel_calibration,
            spl::spl_reset,
            spl::spl_reading,
            spl::spl_history,
            commands::get_session,
            commands::open_billing,
            commands::begin_sign_in,
            commands::complete_sign_in,
            commands::sign_out,
        ])
        .run(tauri::generate_context!())
        .expect("error while running SanctuaryMix");
}

/// Finishes sign-in from the website's sanctuarymix:// link.
async fn finish_sign_in(app: AppHandle, url: String) {
    let result = app.state::<AppState>().auth.complete_sign_in(url).await;
    match result {
        Ok(session) => {
            let _ = app.emit("session", session);
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }
        Err(e) => {
            let _ = app.emit("session-error", e.to_string());
        }
    }
}

/// Re-checks the account and plan at launch and every few hours, but only
/// between services: a plan change never lands mid-service. Offline, the last
/// confirmed plan holds (see `auth::OFFLINE_GRACE_DAYS`).
async fn check_plan(app: AppHandle) {
    loop {
        if commands::in_service(&app).await {
            tokio::time::sleep(PLAN_CHECK_RETRY).await;
            continue;
        }
        let state = app.state::<AppState>();
        let before = state.auth.current_session().await;
        if before.authenticated {
            let after = state.auth.refresh().await;
            if after != before {
                let _ = app.emit("session", after);
            }
        }
        tokio::time::sleep(PLAN_CHECK_EVERY).await;
    }
}
