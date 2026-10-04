//! Room loudness in the app: feeds one input of the metering stream to an
//! [`SplMeter`], pushes `spl` readings to the UI ~10 times a second, runs the
//! guided calibration (`spl-calibration` events), and keeps the source and
//! calibration in the app database.
//!
//! Other parts of the app (auto-mix loudness targets, later) read
//! [`Spl::latest`] rather than the audio.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use audio_engine::{CalibrationProgress, Calibrator, SplMeter};
use mix_core::spl::{
    CalibrationRecord, CalibrationStatus, SplConfig, SplPoint, SplReading, Weighting,
};
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
    /// The audio device and sample rate currently metering, once known.
    device: Option<(String, u32)>,
    meter: Option<SplMeter>,
    latest: Option<SplReading>,
    calibration: Option<Run>,
    blocks: u32,
}

/// A guided calibration in progress.
struct Run {
    calibrator: Calibrator,
    weighting: Weighting,
    reference_db: f32,
}

impl Inner {
    /// Calibrated only while the saved calibration matches what's in use now.
    fn recheck(&mut self) -> bool {
        let Some((device, rate)) = &self.device else {
            return false;
        };
        let still = self
            .config
            .calibration
            .as_ref()
            .is_some_and(|r| r.matches(self.config.source, device, *rate));
        let changed = still != self.config.calibrated;
        self.config.calibrated = still;
        changed
    }

    /// Turns a finished run into the new settings, or the reason it failed.
    fn finish(&mut self, run: &Run, measured_dbfs: f32) -> CalibrationStatus {
        let offset = run.reference_db - measured_dbfs;
        let (Some(source), Some((device, sample_rate))) = (self.config.source, &self.device) else {
            return failed("Input lost. Try again.");
        };
        if !SplConfig::OFFSET_RANGE.contains(&offset) {
            return failed("Reading doesn't match this input. Check the input and number.");
        }
        self.config = SplConfig {
            source: Some(source),
            offset_db: offset,
            calibrated: true,
            calibration: Some(CalibrationRecord {
                source,
                device: device.clone(),
                sample_rate: *sample_rate,
                weighting: run.weighting,
                reference_db: run.reference_db,
                at_ms: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |d| d.as_millis() as u64),
            }),
        };
        CalibrationStatus::Done {
            offset_db: offset,
            config: self.config.clone(),
        }
    }
}

fn failed(reason: &str) -> CalibrationStatus {
    CalibrationStatus::Failed {
        reason: reason.into(),
    }
}

fn status_of(progress: CalibrationProgress) -> CalibrationStatus {
    match progress {
        CalibrationProgress::Listening {
            level_dbfs,
            steady_secs,
            elapsed_secs,
            hold,
        } => CalibrationStatus::Listening {
            level_dbfs,
            steady_secs,
            needed_secs: audio_engine::spl::calibrate::STEADY_SECS,
            elapsed_secs,
            hold: hold.map(str::to_owned),
        },
        CalibrationProgress::Failed { reason } => CalibrationStatus::Failed { reason },
        // Done is turned into settings by `Inner::finish`.
        CalibrationProgress::Done { .. } => failed("Calibration finished unexpectedly."),
    }
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

    /// Records which device is metering. If the calibration was made on
    /// another one, readings go back to uncalibrated (and that is saved).
    pub fn set_device(&self, app: &AppHandle, device: &str, sample_rate: u32) {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            inner.device = Some((device.to_owned(), sample_rate));
            inner.recheck().then(|| inner.config.clone())
        };
        if let Some(config) = changed {
            save_in_background(app, config);
        }
    }

    /// Measures the source input out of one block of captured audio. Called
    /// on the metering thread, never the real-time callback.
    pub fn push_audio(&self, app: &AppHandle, channels: u16, sample_rate: u32, samples: &[f32]) {
        let mut events: (Option<SplReading>, Option<CalibrationStatus>) = (None, None);
        let mut save = None;
        {
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
            let emit = inner.blocks.is_multiple_of(EMIT_EVERY);

            if let Some(mut run) = inner.calibration.take() {
                run.calibrator
                    .feed_interleaved(samples, channels as usize, source as usize);
                match run.calibrator.progress() {
                    CalibrationProgress::Done { measured_dbfs } => {
                        let status = inner.finish(&run, measured_dbfs);
                        if matches!(status, CalibrationStatus::Done { .. }) {
                            save = Some(inner.config.clone());
                        }
                        events.1 = Some(status);
                    }
                    progress @ CalibrationProgress::Failed { .. } => {
                        events.1 = Some(status_of(progress))
                    }
                    progress => {
                        if emit {
                            events.1 = Some(status_of(progress));
                        }
                        inner.calibration = Some(run);
                    }
                }
            }

            if emit || save.is_some() {
                let config = inner.config.clone();
                let reading = inner
                    .meter
                    .as_ref()
                    .and_then(|m| m.reading(&config, source));
                inner.latest = reading;
                events.0 = reading;
            }
        }
        if let Some(config) = save {
            save_in_background(app, config);
        }
        if let Some(status) = events.1 {
            let _ = app.emit("spl-calibration", status);
        }
        if let Some(reading) = events.0 {
            let _ = app.emit("spl", reading);
        }
    }

    /// The most recent reading, or `None` if there's no source or no audio yet.
    pub fn latest(&self) -> Option<SplReading> {
        self.inner.lock().unwrap().latest
    }

    fn config(&self) -> SplConfig {
        self.inner.lock().unwrap().config.clone()
    }

    /// Applies the operator's source and offset. Only a calibration can mark
    /// readings calibrated; typing an offset clears it. A new source starts
    /// the measurement over.
    fn set_config(&self, requested: SplConfig) -> SplConfig {
        let requested = requested.sanitized();
        let mut inner = self.inner.lock().unwrap();
        if inner.config.source != requested.source {
            inner.meter = None;
            inner.latest = None;
            inner.calibration = None;
        }
        if (inner.config.offset_db - requested.offset_db).abs() >= 0.05 {
            inner.config.offset_db = requested.offset_db;
            inner.config.calibration = None;
        }
        inner.config.source = requested.source;
        inner.recheck();
        inner.config.clone()
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

fn write(app: &AppHandle, config: &SplConfig) -> CmdResult<()> {
    app.state::<AppState>()
        .store
        .lock()
        .unwrap()
        .set_setting(SETTINGS_KEY, config)
        .map_err(err)
}

fn save_in_background(app: &AppHandle, config: SplConfig) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(e) = write(&app, &config) {
            log::error!("couldn't save SPL settings: {e}");
        }
    });
}

async fn save(app: AppHandle, config: SplConfig) -> CmdResult<()> {
    tauri::async_runtime::spawn_blocking(move || write(&app, &config))
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
    save(app, applied.clone()).await?;
    Ok(applied)
}

/// Starts a guided calibration against a reference meter (set to slow) or a
/// calibrator. Progress and the result arrive as `spl-calibration` events.
#[tauri::command]
pub async fn spl_calibrate(
    state: State<'_, AppState>,
    weighting: Weighting,
    reference_db: f32,
) -> CmdResult<()> {
    if !(30.0..=140.0).contains(&reference_db) {
        return Err("Enter 30 to 140 dB.".into());
    }
    let mut inner = state.spl.inner.lock().unwrap();
    if inner.config.source.is_none() {
        return Err("Choose an input first.".into());
    }
    let Some((_, sample_rate)) = inner.device else {
        return Err("Start audio first.".into());
    };
    inner.calibration = Some(Run {
        calibrator: Calibrator::new(weighting, sample_rate),
        weighting,
        reference_db,
    });
    Ok(())
}

#[tauri::command]
pub async fn spl_cancel_calibration(state: State<'_, AppState>) -> CmdResult<()> {
    state.spl.inner.lock().unwrap().calibration = None;
    Ok(())
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
