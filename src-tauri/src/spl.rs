//! Room loudness in the app: feeds one input of the metering stream to an
//! [`SplMeter`], pushes `spl` readings to the UI ~10 times a second, and
//! keeps the source and calibration in the app database.
//!
//! Other parts of the app (auto-mix loudness targets, later) read
//! [`Spl::latest`] rather than the audio.

use std::sync::Mutex;

use audio_engine::SplMeter;
use mix_core::spl::{SplConfig, SplPoint, SplReading, Weighting};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

const SETTINGS_KEY: &str = "spl.config";
/// The metering thread hands over audio ~30 times a second; send every third.
const EMIT_EVERY: u32 = 3;

#[derive(Default)]
pub struct Spl {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    config: SplConfig,
    meter: Option<SplMeter>,
    latest: Option<SplReading>,
    blocks: u32,
}

impl Spl {
    pub fn new(config: SplConfig) -> Self {
        Self {
            inner: Mutex::new(Inner {
                config: config.sanitized(),
                ..Inner::default()
            }),
        }
    }

    /// Measures the source input out of one block of captured audio. Called
    /// on the metering thread, never the real-time callback.
    pub fn push_audio(&self, app: &AppHandle, channels: u16, sample_rate: u32, samples: &[f32]) {
        let reading = {
            let mut inner = self.inner.lock().unwrap();
            let Some(source) = inner.config.source else {
                return;
            };
            if source >= channels {
                return;
            }
            let meter = match &mut inner.meter {
                Some(m) if m.sample_rate() == sample_rate => m,
                slot => slot.insert(SplMeter::new(sample_rate)),
            };
            meter.process_interleaved(samples, channels as usize, source as usize);
            inner.blocks = inner.blocks.wrapping_add(1);
            if !inner.blocks.is_multiple_of(EMIT_EVERY) {
                return;
            }
            let config = inner.config;
            let reading = inner
                .meter
                .as_ref()
                .and_then(|m| m.reading(&config, source));
            inner.latest = reading;
            reading
        };
        if let Some(reading) = reading {
            let _ = app.emit("spl", reading);
        }
    }

    /// The most recent reading, or `None` if there's no source or no audio yet.
    pub fn latest(&self) -> Option<SplReading> {
        self.inner.lock().unwrap().latest
    }

    fn config(&self) -> SplConfig {
        self.inner.lock().unwrap().config
    }

    /// Applies new settings. A new source starts the measurement over.
    fn set_config(&self, config: SplConfig) -> SplConfig {
        let config = config.sanitized();
        let mut inner = self.inner.lock().unwrap();
        if inner.config.source != config.source {
            inner.meter = None;
            inner.latest = None;
        }
        inner.config = config;
        config
    }
}

/// Reads saved SPL settings, falling back to off.
pub fn load_config(store: &store::Store) -> SplConfig {
    store
        .get_setting::<SplConfig>(SETTINGS_KEY)
        .unwrap_or_else(|e| {
            log::warn!("ignoring saved SPL settings: {e}");
            None
        })
        .unwrap_or_default()
}

async fn save(app: AppHandle, config: SplConfig) -> CmdResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>()
            .store
            .lock()
            .unwrap()
            .set_setting(SETTINGS_KEY, &config)
            .map_err(err)
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub async fn spl_get_config(state: State<'_, AppState>) -> CmdResult<SplConfig> {
    Ok(state.spl.config())
}

/// Saves the source and offset. Returns them as the meter will use them.
#[tauri::command]
pub async fn spl_set_config(app: AppHandle, config: SplConfig) -> CmdResult<SplConfig> {
    let applied = app.state::<AppState>().spl.set_config(config);
    save(app, applied).await?;
    Ok(applied)
}

/// Sets the offset so the current slow reading matches a reference meter
/// (or a 94 dB calibrator) held next to the measurement mic.
#[tauri::command]
pub async fn spl_calibrate(
    app: AppHandle,
    weighting: Weighting,
    reference_db: f32,
) -> CmdResult<SplConfig> {
    if !(30.0..=140.0).contains(&reference_db) {
        return Err("Enter the reference meter's reading, between 30 and 140 dB.".into());
    }
    let applied = {
        let spl = &app.state::<AppState>().spl;
        let mut inner = spl.inner.lock().unwrap();
        let offset = inner
            .meter
            .as_ref()
            .and_then(|m| m.calibration_offset(weighting, reference_db))
            .ok_or(
                "The measurement input is too quiet to calibrate. Check the source and play pink noise or a calibrator, then try again.",
            )?;
        let next = SplConfig {
            offset_db: offset,
            calibrated: true,
            ..inner.config
        }
        .sanitized();
        if next.offset_db != offset {
            return Err("That reading is too far from what this input hears. Check you picked the measurement mic.".into());
        }
        inner.config = next;
        next
    };
    save(app, applied).await?;
    Ok(applied)
}

/// Clears history, Leq, maximum and peak, e.g. at the start of a service.
#[tauri::command]
pub async fn spl_reset(state: State<'_, AppState>) -> CmdResult<()> {
    let mut inner = state.spl.inner.lock().unwrap();
    if let Some(m) = &mut inner.meter {
        m.reset();
    }
    inner.latest = None;
    Ok(())
}

#[tauri::command]
pub async fn spl_reading(state: State<'_, AppState>) -> CmdResult<Option<SplReading>> {
    Ok(state.spl.latest())
}

/// One-second Leq points for the last `seconds`, oldest first.
#[tauri::command]
pub async fn spl_history(state: State<'_, AppState>, seconds: u32) -> CmdResult<Vec<SplPoint>> {
    let inner = state.spl.inner.lock().unwrap();
    Ok(inner
        .meter
        .as_ref()
        .map(|m| m.history(&inner.config, seconds as usize))
        .unwrap_or_default())
}
