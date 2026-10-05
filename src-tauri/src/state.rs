use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use audio_engine::MeterHandle;
use auth::{AuthProvider, KeychainStore, SupabaseAuth, SupabaseConfig};
use automix::{Adjustment, AutoMixConfig, AutoMixHandle, AutoMixStatus, FaderSink, Observer};
use console::{ConsoleAdapter, ConsoleError};
use listen::{ListenTarget, Listener, Models};
use mix_core::{ChangeSource, ChannelId, ConsoleEvent};
use store::Store;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager};

use crate::control::ControlBus;
use crate::playback::Playback;
use crate::recording::Recordings;
use crate::spl::Spl;

pub struct AppState {
    pub metering: Mutex<Option<MeterHandle>>,
    pub console: tokio::sync::Mutex<Option<Box<dyn ConsoleAdapter>>>,
    /// Forwards console events to the UI; replaced on every connect.
    pub console_forwarder: Mutex<Option<JoinHandle<()>>>,
    pub auth: Box<dyn AuthProvider>,
    pub store: Arc<Mutex<Store>>,
    pub automix: AutoMixHandle,
    /// On-device listening models; `None` if they couldn't load (auto-mix
    /// then works from levels alone).
    pub listener: Option<Arc<Listener>>,
    /// Every control change from every source; recordings listen here.
    pub control: ControlBus,
    /// `None` only if the recordings folder couldn't be opened.
    pub recordings: Option<Recordings>,
    pub playback: Playback,
    /// Room loudness from the measurement input.
    pub spl: Spl,
}

impl AppState {
    /// Opens the local database and starts the auto-mix loop (switched off).
    pub fn new(app: &AppHandle) -> Self {
        let store = Arc::new(Mutex::new(open_store(app)));
        let config = store
            .lock()
            .unwrap()
            .get_setting::<AutoMixConfig>(store::keys::AUTOMIX_CONFIG)
            .unwrap_or_else(|e| {
                log::warn!("ignoring saved auto-mix settings: {e}");
                None
            })
            .unwrap_or_default();
        let spl = Spl::new(crate::spl::load_config(&store.lock().unwrap()));
        let targets = listen_targets(&config);
        let (automix, task) = automix::start(
            config,
            Arc::new(ConsoleSink(app.clone())),
            TauriObserver {
                app: app.clone(),
                store: store.clone(),
            },
        );
        tauri::async_runtime::spawn(task);
        let listener = start_listening(&automix, targets);
        Self {
            metering: Mutex::new(None),
            console: tokio::sync::Mutex::new(None),
            console_forwarder: Mutex::new(None),
            // Opens with the sign-in saved in the keychain; checks it online after launch.
            auth: Box::new(SupabaseAuth::new(
                SupabaseConfig::sanctuarymix(),
                Box::new(KeychainStore::new()),
            )),
            store,
            automix,
            listener,
            control: ControlBus::default(),
            recordings: Recordings::open(app)
                .inspect_err(|e| log::error!("recordings are off this run: {e}"))
                .ok(),
            playback: Playback::default(),
            spl,
        }
    }
}

/// Loads the listening models and points them at auto-mix's channels.
fn start_listening(automix: &AutoMixHandle, targets: Vec<ListenTarget>) -> Option<Arc<Listener>> {
    let models = match Models::load() {
        Ok(m) => m,
        Err(e) => {
            log::error!("listening models didn't load, auto-mix will use levels only: {e}");
            return None;
        }
    };
    let automix = automix.clone();
    let listener = Listener::start(models, move |frame| automix.push_hearing(frame));
    listener.set_targets(targets);
    Some(Arc::new(listener))
}

/// The channels to listen to for this auto-mix setup. Voice mics also get the voice detector.
pub fn listen_targets(config: &AutoMixConfig) -> Vec<ListenTarget> {
    if !config.listen {
        return Vec::new();
    }
    config
        .channels
        .iter()
        .map(|c| ListenTarget {
            channel: c.channel,
            voice: c.role.is_vocal(),
        })
        .collect()
}

/// `<app data>/sanctuarymix.db`, or an in-memory database if that can't be opened.
fn open_store(app: &AppHandle) -> Store {
    let opened = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())
        .and_then(|dir| {
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            Store::open(&dir.join("sanctuarymix.db")).map_err(|e| e.to_string())
        });
    opened.unwrap_or_else(|e| {
        log::error!("couldn't open the local database, settings won't be saved this run: {e}");
        Store::open_in_memory().expect("in-memory SQLite")
    })
}

/// Auto-mix's only way to the console: input faders, nothing else.
struct ConsoleSink(AppHandle);

#[async_trait]
impl FaderSink for ConsoleSink {
    async fn set_fader(&self, channel: u16, db: f32) -> Result<(), String> {
        let state = self.0.state::<AppState>();
        let slot = state.console.lock().await;
        let adapter = slot.as_ref().ok_or(ConsoleError::NotConnected);
        adapter
            .map_err(|e| e.to_string())?
            .set_fader(ChannelId::input(channel), Some(db))
            .await
            .map_err(|e| e.to_string())?;
        state.control.publish(
            ChangeSource::Assist,
            ConsoleEvent::Fader {
                id: ChannelId::input(channel),
                db: Some(db),
            },
        );
        Ok(())
    }
}

/// Sends status and adjustments to the UI and writes every adjustment to the log.
struct TauriObserver {
    app: AppHandle,
    store: Arc<Mutex<Store>>,
}

impl Observer for TauriObserver {
    fn status(&self, status: &AutoMixStatus) {
        let _ = self.app.emit("automix", status);
    }

    fn adjustment(&self, adjustment: &Adjustment) {
        if let Err(e) = self.store.lock().unwrap().append_adjustment(adjustment) {
            log::error!("couldn't log an auto-mix adjustment: {e}");
        }
        let _ = self.app.emit("automix-adjustment", adjustment);
    }
}
