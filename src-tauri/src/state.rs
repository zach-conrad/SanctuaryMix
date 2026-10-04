use std::sync::Mutex;

use audio_engine::MeterHandle;
use auth::{AuthProvider, LocalGuest};
use console::ConsoleAdapter;
use tauri::async_runtime::JoinHandle;

pub struct AppState {
    pub metering: Mutex<Option<MeterHandle>>,
    pub console: tokio::sync::Mutex<Option<Box<dyn ConsoleAdapter>>>,
    /// Forwards console events to the UI; replaced on every connect.
    pub console_forwarder: Mutex<Option<JoinHandle<()>>>,
    pub auth: Box<dyn AuthProvider>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            metering: Mutex::new(None),
            console: tokio::sync::Mutex::new(None),
            console_forwarder: Mutex::new(None),
            auth: Box::new(LocalGuest),
        }
    }
}
