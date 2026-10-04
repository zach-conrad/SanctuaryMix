//! Tauri shell: wires the Rust crates to the React UI.
//!
//! The UI calls the `#[tauri::command]`s in [`commands`] and listens for these
//! events: `meters` (a [`mix_core::MeterFrame`] ~30x a second), `console`
//! (a [`mix_core::ConsoleEvent`] whenever the desk changes), `automix` (an
//! [`automix::AutoMixStatus`] a few times a second) and `automix-adjustment`
//! (an [`automix::Adjustment`] for every auto-mix move or takeover),
//! `recording` (recorder status about once a second), `playback` (the
//! transport about 10x a second) and `replay` (sending recorded moves).

mod commands;
mod control;
mod playback;
mod recording;
mod state;

use state::AppState;
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .setup(|app| {
            let state = AppState::new(app.handle());
            app.manage(state);
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
            commands::get_session,
            commands::begin_sign_in,
            commands::complete_sign_in,
            commands::sign_out,
        ])
        .run(tauri::generate_context!())
        .expect("error while running SanctuaryMix");
}
