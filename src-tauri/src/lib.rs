//! Tauri shell: wires the Rust crates to the React UI.
//!
//! The UI calls the `#[tauri::command]`s in [`commands`] and listens for two
//! events: `meters` (a [`mix_core::MeterFrame`] ~30x a second) and `console`
//! (a [`mix_core::ConsoleEvent`] whenever the desk changes).

mod commands;
mod state;

use state::AppState;

pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::list_audio_devices,
            commands::start_metering,
            commands::stop_metering,
            commands::connect_console,
            commands::disconnect_console,
            commands::set_fader,
            commands::set_mute,
            commands::request_channel_names,
            commands::get_session,
            commands::begin_sign_in,
            commands::complete_sign_in,
            commands::sign_out,
        ])
        .run(tauri::generate_context!())
        .expect("error while running SanctuaryMix");
}
