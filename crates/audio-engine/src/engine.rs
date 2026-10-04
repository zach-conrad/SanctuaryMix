use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use mix_core::MeterFrame;

use crate::devices::find_input_device;
use crate::meter::MeterBank;
use crate::{AudioError, Result};

/// What was actually opened, which may differ from what was asked for.
struct Opened {
    device_name: String,
    channels: u16,
    sample_rate: u32,
}

const FRAME_INTERVAL: Duration = Duration::from_millis(33);

/// Running capture. Dropping it stops the stream.
pub struct MeterHandle {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    pub device_name: String,
    pub channels: u16,
    pub sample_rate: u32,
}

impl Drop for MeterHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Opens `device_name` (or the system default) with as many input channels as
/// it offers and calls `on_frame` ~30 times a second with fresh levels.
pub fn start_metering(
    device_name: Option<String>,
    on_frame: impl Fn(MeterFrame) + Send + 'static,
) -> Result<MeterHandle> {
    let stop = Arc::new(AtomicBool::new(false));
    let (ready_tx, ready_rx) = mpsc::channel::<Result<Opened>>();
    let thread_stop = stop.clone();

    // cpal streams are not Send on every platform, so one thread owns the stream for its whole life.
    let thread = std::thread::Builder::new()
        .name("sanctuarymix-audio".into())
        .spawn(move || {
            let opened = open_stream(device_name.as_deref());
            let (stream, bank, info) = match opened {
                Ok(v) => v,
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            let sample_rate = info.sample_rate;
            let _ = ready_tx.send(Ok(info));
            while !thread_stop.load(Ordering::SeqCst) {
                std::thread::sleep(FRAME_INTERVAL);
                on_frame(bank.take_frame(sample_rate));
            }
            drop(stream);
        })
        .map_err(|e| AudioError::Backend(e.to_string()))?;

    match ready_rx.recv() {
        Ok(Ok(Opened {
            device_name,
            channels,
            sample_rate,
        })) => {
            log::info!("metering {channels} ch from {device_name:?} at {sample_rate} Hz");
            Ok(MeterHandle {
                stop,
                thread: Some(thread),
                device_name,
                channels,
                sample_rate,
            })
        }
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e)
        }
        Err(_) => Err(AudioError::Backend("audio thread exited".into())),
    }
}

fn open_stream(device_name: Option<&str>) -> Result<(cpal::Stream, Arc<MeterBank>, Opened)> {
    let device = find_input_device(device_name)?;
    let name = device.name()?;
    let default = device.default_input_config()?;
    let sample_rate = default.sample_rate();

    // Dante Virtual Soundcard reports a 2-channel default; ask for everything it has.
    let best = device
        .supported_input_configs()?
        .filter(|c| c.min_sample_rate() <= sample_rate && sample_rate <= c.max_sample_rate())
        .max_by_key(|c| (c.channels(), c.sample_format() == SampleFormat::F32))
        .map(|c| c.with_sample_rate(sample_rate))
        .unwrap_or(default);

    let format = best.sample_format();
    let config: StreamConfig = best.into();
    let bank = Arc::new(MeterBank::new(config.channels as usize));
    let err_fn = |e| log::error!("audio stream error: {e}");

    let stream = match format {
        SampleFormat::F32 => {
            let bank = bank.clone();
            device.build_input_stream(
                &config,
                move |d: &[f32], _| bank.push_interleaved(d),
                err_fn,
                None,
            )?
        }
        SampleFormat::I16 => build_converting::<i16>(&device, &config, bank.clone(), err_fn)?,
        SampleFormat::I32 => build_converting::<i32>(&device, &config, bank.clone(), err_fn)?,
        other => return Err(AudioError::UnsupportedFormat(format!("{other:?}"))),
    };
    stream.play()?;
    Ok((
        stream,
        bank,
        Opened {
            device_name: name,
            channels: config.channels,
            sample_rate: config.sample_rate.0,
        },
    ))
}

fn build_converting<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    bank: Arc<MeterBank>,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    // Allocated once; resized only if the host hands us a bigger buffer than before.
    let mut scratch: Vec<f32> = Vec::with_capacity(8192);
    Ok(device.build_input_stream(
        config,
        move |d: &[T], _| {
            scratch.clear();
            scratch.extend(
                d.iter()
                    .map(|&s| <f32 as cpal::FromSample<T>>::from_sample_(s)),
            );
            bank.push_interleaved(&scratch);
        },
        err_fn,
        None,
    )?)
}
