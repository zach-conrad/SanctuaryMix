//! Playing a recording to any output device.
//!
//! Output only, so no input permission is involved. Dante Virtual Soundcard
//! shows up as an output device too, which is how a recording reaches a PA.
//!
//! One thread ("sanctuarymix-playback") owns the cpal stream (streams are not
//! `Send` everywhere), reads and resamples the WAV, and reports the position.
//! The output callback only copies stereo frames out of a lock-free ring.
//! A seek hands the callback a fresh ring through a slot it `try_lock`s and
//! swaps with; the old ring goes back in the slot so it is freed off the audio
//! thread.

use std::io::{Read, Seek};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, StreamConfig};
use rtrb::{Consumer, Producer, RingBuffer};
use serde::Serialize;

use crate::{AudioError, Result};

const POSITION_INTERVAL: Duration = Duration::from_millis(100);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
/// Read-ahead in seconds of output audio.
const RING_SECONDS: f64 = 0.5;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputDeviceInfo {
    pub name: String,
    pub max_output_channels: u16,
    pub is_default: bool,
    /// Dante Virtual Soundcard or a Dante interface, e.g. to feed a PA.
    pub is_dante: bool,
}

pub fn list_output_devices() -> Result<Vec<OutputDeviceInfo>> {
    let host = cpal::default_host();
    let default_name = host.default_output_device().and_then(|d| d.name().ok());
    let mut out = Vec::new();
    for device in host.output_devices()? {
        let Ok(name) = device.name() else { continue };
        let Ok(default_config) = device.default_output_config() else {
            continue;
        };
        let max_output_channels = device
            .supported_output_configs()
            .map(|configs| configs.map(|c| c.channels()).max().unwrap_or(0))
            .unwrap_or(default_config.channels());
        out.push(OutputDeviceInfo {
            is_default: default_name.as_deref() == Some(name.as_str()),
            is_dante: name.to_lowercase().contains("dante"),
            name,
            max_output_channels,
        });
    }
    // Same order as inputs: Dante first, then the system default, then alphabetical.
    out.sort_by(|a, b| {
        b.is_dante
            .cmp(&a.is_dante)
            .then(b.is_default.cmp(&a.is_default))
            .then(a.name.cmp(&b.name))
    });
    Ok(out)
}

fn find_output_device(name: Option<&str>) -> Result<cpal::Device> {
    let host = cpal::default_host();
    match name {
        None => host
            .default_output_device()
            .ok_or(AudioError::NoDefaultOutputDevice),
        Some(wanted) => host
            .output_devices()?
            .find(|d| d.name().map(|n| n == wanted).unwrap_or(false))
            .ok_or_else(|| AudioError::OutputDeviceNotFound(wanted.to_string())),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackPosition {
    pub position_ms: u64,
    pub duration_ms: u64,
    pub playing: bool,
}

/// A stretch of audio starting at `start_us`, as handed to the callback.
struct Segment {
    consumer: Consumer<f32>,
    start_us: u64,
    played: u64,
}

struct Shared {
    playing: AtomicBool,
    ended: AtomicBool,
    position_us: AtomicU64,
    /// A new segment while `pending_new` is set; otherwise a retired one (or none).
    pending: Mutex<Option<Segment>>,
    pending_new: AtomicBool,
}

impl Shared {
    fn new() -> Self {
        Self {
            playing: AtomicBool::new(false),
            ended: AtomicBool::new(false),
            position_us: AtomicU64::new(0),
            pending: Mutex::new(None),
            pending_new: AtomicBool::new(false),
        }
    }

    /// Creates a ring for audio from `start_us` on and offers it to the callback.
    fn install_segment(&self, start_us: u64, capacity: usize) -> Producer<f32> {
        let (producer, consumer) = RingBuffer::new(capacity);
        let mut slot = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        *slot = Some(Segment {
            consumer,
            start_us,
            played: 0,
        });
        self.pending_new.store(true, Ordering::SeqCst);
        producer
    }

    /// Frees a segment the callback swapped out.
    fn drop_retired(&self) {
        if let Ok(mut slot) = self.pending.try_lock() {
            if !self.pending_new.load(Ordering::SeqCst) {
                slot.take();
            }
        }
    }
}

/// Everything the output callback touches.
struct OutputState {
    shared: Arc<Shared>,
    current: Option<Segment>,
    out_channels: usize,
    rate: u64,
}

impl OutputState {
    /// Called from the output callback. Never blocks, allocates or frees.
    fn render<T: Sample + FromSample<f32>>(&mut self, out: &mut [T]) {
        let shared = &*self.shared;
        if shared.pending_new.load(Ordering::SeqCst) {
            if let Ok(mut slot) = shared.pending.try_lock() {
                std::mem::swap(&mut self.current, &mut *slot);
                shared.pending_new.store(false, Ordering::SeqCst);
            }
        }
        let ch = self.out_channels.max(1);
        let frames = out.len() / ch;
        let mut done = 0;
        if let (true, Some(seg)) = (shared.playing.load(Ordering::SeqCst), self.current.as_mut()) {
            let n = (seg.consumer.slots() / 2).min(frames);
            if let Ok(chunk) = seg.consumer.read_chunk(n * 2) {
                let (a, b) = chunk.as_slices();
                // The ring only ever holds whole stereo frames, so the split is aligned.
                let src = a.as_chunks::<2>().0.iter().chain(b.as_chunks::<2>().0);
                for (frame, lr) in out.chunks_exact_mut(ch).zip(src) {
                    write_frame(frame, lr[0], lr[1]);
                }
                chunk.commit_all();
                done = n;
                seg.played += n as u64;
                shared.position_us.store(
                    seg.start_us + seg.played * 1_000_000 / self.rate,
                    Ordering::Relaxed,
                );
            }
            let finished = seg.consumer.is_abandoned() && seg.consumer.is_empty();
            // A seek may already be waiting; then this is not the end.
            if done < frames && finished && !shared.pending_new.load(Ordering::SeqCst) {
                shared.playing.store(false, Ordering::SeqCst);
                shared.ended.store(true, Ordering::SeqCst);
            }
        }
        for s in &mut out[done * ch..] {
            *s = T::EQUILIBRIUM;
        }
    }
}

/// Stereo to the first two outputs; mono devices get the sum.
fn write_frame<T: Sample + FromSample<f32>>(frame: &mut [T], l: f32, r: f32) {
    match frame {
        [only] => *only = T::from_sample((l + r) * 0.5),
        [a, b, rest @ ..] => {
            *a = T::from_sample(l);
            *b = T::from_sample(r);
            for s in rest {
                *s = T::EQUILIBRIUM;
            }
        }
        [] => {}
    }
}

/// Reads a mono or stereo WAV as stereo f32 frames.
struct Source<R: Read> {
    reader: hound::WavReader<R>,
    stereo: bool,
    float: bool,
    scale: f32,
}

impl<R: Read + Seek> Source<R> {
    fn new(reader: hound::WavReader<R>) -> Result<Self> {
        let spec = reader.spec();
        if !(1..=2).contains(&spec.channels) {
            return Err(AudioError::UnsupportedFormat(format!(
                "{}-channel WAV (only mono and stereo play)",
                spec.channels
            )));
        }
        Ok(Self {
            stereo: spec.channels == 2,
            float: spec.sample_format == hound::SampleFormat::Float,
            scale: 1.0 / (1u64 << (spec.bits_per_sample.clamp(1, 32) - 1)) as f32,
            reader,
        })
    }

    fn sample(&mut self) -> Option<f32> {
        let s = if self.float {
            self.reader.samples::<f32>().next()?
        } else {
            self.reader
                .samples::<i32>()
                .next()?
                .map(|s| s as f32 * self.scale)
        };
        // A damaged tail (say, from a crash mid-write) just ends playback.
        s.map_err(|e| log::warn!("playback read error: {e}")).ok()
    }

    fn next_frame(&mut self) -> Option<[f32; 2]> {
        let l = self.sample()?;
        let r = if self.stereo { self.sample()? } else { l };
        Some([l, r])
    }

    fn seek(&mut self, frame: u32) {
        let frame = frame.min(self.reader.duration());
        if let Err(e) = self.reader.seek(frame) {
            log::warn!("playback seek failed: {e}");
        }
    }
}

/// Linear interpolation from the file's rate to the device's. Good enough to
/// listen back; the files themselves are never resampled.
struct Linear {
    step: f64,
    t: f64,
    a: [f32; 2],
    b: Option<[f32; 2]>,
    primed: bool,
}

impl Linear {
    fn new(from_rate: u32, to_rate: u32) -> Self {
        Self {
            step: from_rate as f64 / to_rate as f64,
            t: 0.0,
            a: [0.0; 2],
            b: None,
            primed: false,
        }
    }

    /// Forgets the previous position, after a seek.
    fn reset(&mut self) {
        self.t = 0.0;
        self.b = None;
        self.primed = false;
    }

    fn next(&mut self, mut src: impl FnMut() -> Option<[f32; 2]>) -> Option<[f32; 2]> {
        if !self.primed {
            self.a = src()?;
            self.b = src();
            self.primed = true;
        }
        while self.t >= 1.0 {
            self.a = self.b?;
            self.b = src();
            self.t -= 1.0;
        }
        let out = match self.b {
            Some(b) => {
                let t = self.t as f32;
                [
                    self.a[0] + (b[0] - self.a[0]) * t,
                    self.a[1] + (b[1] - self.a[1]) * t,
                ]
            }
            None => self.a,
        };
        self.t += self.step;
        Some(out)
    }
}

enum Cmd {
    Seek { ms: u64, play: bool },
    Shutdown,
}

/// A loaded recording. Dropping it stops the output stream.
pub struct Player {
    shared: Arc<Shared>,
    cmds: mpsc::Sender<Cmd>,
    thread: Option<JoinHandle<()>>,
    duration_ms: u64,
    pub device_name: String,
    /// The output device's rate; the file is resampled to it if they differ.
    pub sample_rate: u32,
}

impl Player {
    /// Opens `path` on `device` (or the system default output), paused at the
    /// start. `on_position` is called about 10 times a second from a normal
    /// thread.
    pub fn open(
        path: &Path,
        device: Option<String>,
        on_position: impl Fn(PlaybackPosition) + Send + 'static,
    ) -> Result<Player> {
        let source = Source::new(hound::WavReader::open(path)?)?;
        let spec = source.reader.spec();
        let duration_ms = u64::from(source.reader.duration()) * 1000 / u64::from(spec.sample_rate);
        let shared = Arc::new(Shared::new());
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(String, u32)>>();
        let thread_shared = shared.clone();

        let thread = std::thread::Builder::new()
            .name("sanctuarymix-playback".into())
            .spawn(move || {
                let (stream, rate) =
                    match open_output(device.as_deref(), spec.sample_rate, thread_shared.clone()) {
                        Ok((stream, name, rate)) => {
                            let _ = ready_tx.send(Ok((name, rate)));
                            (stream, rate)
                        }
                        Err(e) => {
                            let _ = ready_tx.send(Err(e));
                            return;
                        }
                    };
                run_reader(
                    source,
                    spec.sample_rate,
                    rate,
                    duration_ms,
                    &thread_shared,
                    cmd_rx,
                    on_position,
                );
                drop(stream);
            })
            .map_err(|e| AudioError::Backend(e.to_string()))?;

        match ready_rx.recv() {
            Ok(Ok((device_name, sample_rate))) => {
                log::info!(
                    "playing {path:?} ({} Hz) on {device_name:?} at {sample_rate} Hz",
                    spec.sample_rate
                );
                Ok(Player {
                    shared,
                    cmds: cmd_tx,
                    thread: Some(thread),
                    duration_ms,
                    device_name,
                    sample_rate,
                })
            }
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => Err(AudioError::Backend("playback thread exited".into())),
        }
    }

    /// Starts or resumes; from the end it starts over.
    pub fn play(&self) {
        if self.shared.ended.load(Ordering::SeqCst) {
            let _ = self.cmds.send(Cmd::Seek { ms: 0, play: true });
        } else {
            self.shared.playing.store(true, Ordering::SeqCst);
        }
    }

    pub fn pause(&self) {
        self.shared.playing.store(false, Ordering::SeqCst);
    }

    pub fn seek(&self, ms: u64) {
        let ms = ms.min(self.duration_ms);
        self.shared.position_us.store(ms * 1000, Ordering::Relaxed);
        // Re-asserted after the new segment is in, in case the old one ran out meanwhile.
        let play = self.is_playing();
        let _ = self.cmds.send(Cmd::Seek { ms, play });
    }

    pub fn position_ms(&self) -> u64 {
        position_ms(&self.shared, self.duration_ms)
    }

    pub fn duration_ms(&self) -> u64 {
        self.duration_ms
    }

    pub fn is_playing(&self) -> bool {
        self.shared.playing.load(Ordering::SeqCst)
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.cmds.send(Cmd::Shutdown);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn position_ms(shared: &Shared, duration_ms: u64) -> u64 {
    if shared.ended.load(Ordering::SeqCst) {
        duration_ms
    } else {
        (shared.position_us.load(Ordering::Relaxed) / 1000).min(duration_ms)
    }
}

fn open_output(
    name: Option<&str>,
    file_rate: u32,
    shared: Arc<Shared>,
) -> Result<(cpal::Stream, String, u32)> {
    let device = find_output_device(name)?;
    let device_name = device.name()?;
    let default = device.default_output_config()?;
    // Use the file's rate when the device can run at it, so nothing is resampled.
    let wanted = cpal::SampleRate(file_rate);
    let best = device
        .supported_output_configs()?
        .find(|c| {
            c.channels() == default.channels()
                && c.sample_format() == default.sample_format()
                && c.min_sample_rate() <= wanted
                && wanted <= c.max_sample_rate()
        })
        .map(|c| c.with_sample_rate(wanted))
        .unwrap_or(default);

    let format = best.sample_format();
    let config: StreamConfig = best.into();
    let state = OutputState {
        shared,
        current: None,
        out_channels: config.channels as usize,
        rate: u64::from(config.sample_rate.0),
    };
    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, &config, state)?,
        SampleFormat::I16 => build::<i16>(&device, &config, state)?,
        SampleFormat::I32 => build::<i32>(&device, &config, state)?,
        SampleFormat::U16 => build::<u16>(&device, &config, state)?,
        other => return Err(AudioError::UnsupportedFormat(format!("{other:?}"))),
    };
    stream.play()?;
    Ok((stream, device_name, config.sample_rate.0))
}

fn build<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: &StreamConfig,
    mut state: OutputState,
) -> Result<cpal::Stream> {
    Ok(device.build_output_stream(
        config,
        move |out: &mut [T], _| state.render(out),
        |e| log::error!("playback stream error: {e}"),
        None,
    )?)
}

/// Keeps the ring full, handles seeks and reports the position until shutdown.
fn run_reader<R: Read + Seek>(
    mut source: Source<R>,
    file_rate: u32,
    device_rate: u32,
    duration_ms: u64,
    shared: &Shared,
    cmds: mpsc::Receiver<Cmd>,
    on_position: impl Fn(PlaybackPosition),
) {
    // Even, so the ring always holds whole stereo frames.
    let capacity = ((device_rate as f64 * RING_SECONDS) as usize).max(64) * 2;
    let mut resampler = Linear::new(file_rate, device_rate);
    let mut producer = Some(shared.install_segment(0, capacity));
    let mut last_report: Option<Instant> = None;
    loop {
        match cmds.try_recv() {
            Ok(Cmd::Seek { ms, play }) => {
                let frame = ms * u64::from(file_rate) / 1000;
                source.seek(frame.min(u64::from(u32::MAX)) as u32);
                resampler.reset();
                shared.position_us.store(ms * 1000, Ordering::Relaxed);
                producer = Some(shared.install_segment(ms * 1000, capacity));
                shared.ended.store(false, Ordering::SeqCst);
                if play {
                    shared.playing.store(true, Ordering::SeqCst);
                }
                continue;
            }
            Ok(Cmd::Shutdown) | Err(TryRecvError::Disconnected) => break,
            Err(TryRecvError::Empty) => {}
        }

        let mut pushed = false;
        let mut at_end = false;
        if let Some(p) = producer.as_mut() {
            for _ in 0..p.slots() / 2 {
                match resampler.next(|| source.next_frame()) {
                    Some(frame) => {
                        let _ = p.push_entire_slice(&frame);
                        pushed = true;
                    }
                    None => {
                        at_end = true;
                        break;
                    }
                }
            }
        }
        if at_end {
            // Dropping the producer is how the callback learns the file ended.
            producer = None;
        }
        shared.drop_retired();

        if last_report.is_none_or(|t| t.elapsed() >= POSITION_INTERVAL) {
            on_position(PlaybackPosition {
                position_ms: position_ms(shared, duration_ms),
                duration_ms,
                playing: shared.playing.load(Ordering::SeqCst),
            });
            last_report = Some(Instant::now());
        }
        if !pushed {
            std::thread::sleep(POLL_INTERVAL);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn wav(channels: u16, rate: u32, samples: &[i16]) -> hound::WavReader<Cursor<Vec<u8>>> {
        let spec = hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut buf = Cursor::new(Vec::new());
        let mut w = hound::WavWriter::new(&mut buf, spec).unwrap();
        for &s in samples {
            w.write_sample(s).unwrap();
        }
        w.finalize().unwrap();
        buf.set_position(0);
        hound::WavReader::new(buf).unwrap()
    }

    fn collect(mut r: Linear, frames: &[f32]) -> Vec<f32> {
        let mut it = frames.iter().map(|&v| [v, -v]);
        std::iter::from_fn(|| r.next(|| it.next()))
            .map(|f| f[0])
            .collect()
    }

    #[test]
    fn resampler_passes_through_at_equal_rates() {
        let r = Linear::new(48_000, 48_000);
        assert_eq!(collect(r, &[0.1, 0.2, 0.3]), vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn resampler_interpolates_up_and_skips_down() {
        assert_eq!(
            collect(Linear::new(24_000, 48_000), &[0.0, 1.0]),
            vec![0.0, 0.5, 1.0, 1.0]
        );
        assert_eq!(
            collect(Linear::new(96_000, 48_000), &[0.0, 0.1, 0.2, 0.3]),
            vec![0.0, 0.2]
        );
    }

    #[test]
    fn mono_files_play_on_both_sides() {
        let mut s = Source::new(wav(1, 44_100, &[16_384, -8_192])).unwrap();
        assert_eq!(s.next_frame(), Some([0.5, 0.5]));
        assert_eq!(s.next_frame(), Some([-0.25, -0.25]));
        assert_eq!(s.next_frame(), None);
        s.seek(1);
        assert_eq!(s.next_frame(), Some([-0.25, -0.25]));
    }

    #[test]
    fn rejects_multichannel_files() {
        assert!(Source::new(wav(3, 48_000, &[0, 0, 0])).is_err());
    }

    #[test]
    fn callback_maps_stereo_and_tracks_position() {
        let shared = Arc::new(Shared::new());
        let mut state = OutputState {
            shared: shared.clone(),
            current: None,
            out_channels: 4,
            rate: 1000,
        };
        let mut p = shared.install_segment(2_000_000, 16);
        p.push_entire_slice(&[0.5, -0.5, 0.25, -0.25]).unwrap();

        // Paused: silence, nothing consumed.
        let mut out = [1.0f32; 12];
        state.render(&mut out);
        assert!(out.iter().all(|&s| s == 0.0));

        shared.playing.store(true, Ordering::SeqCst);
        state.render(&mut out);
        assert_eq!(&out[..8], &[0.5, -0.5, 0.0, 0.0, 0.25, -0.25, 0.0, 0.0]);
        assert_eq!(&out[8..], &[0.0; 4]);
        assert_eq!(shared.position_us.load(Ordering::Relaxed), 2_002_000);
        assert!(shared.is_playing_and_not_ended());

        // The reader finished the file: the next empty render ends playback.
        drop(p);
        state.render(&mut out);
        assert!(shared.ended.load(Ordering::SeqCst));
        assert!(!shared.playing.load(Ordering::SeqCst));
        assert_eq!(position_ms(&shared, 9_000), 9_000);
    }

    #[test]
    fn seek_swaps_segments_and_retires_the_old_one() {
        let shared = Arc::new(Shared::new());
        let mut state = OutputState {
            shared: shared.clone(),
            current: None,
            out_channels: 1,
            rate: 1000,
        };
        shared.playing.store(true, Ordering::SeqCst);
        let mut first = shared.install_segment(0, 8);
        first.push_entire_slice(&[0.2, 0.2]).unwrap();
        let mut out = [0.0f32; 1];
        state.render(&mut out);
        assert_eq!(out[0], 0.2);

        let mut second = shared.install_segment(5_000_000, 8);
        second.push_entire_slice(&[0.4, 0.8]).unwrap();
        state.render(&mut out);
        assert!((out[0] - 0.6).abs() < 1e-6, "mono gets the average");
        assert_eq!(shared.position_us.load(Ordering::Relaxed), 5_001_000);

        // The old segment sits in the slot until the reader frees it.
        assert!(shared.pending.lock().unwrap().is_some());
        shared.drop_retired();
        assert!(shared.pending.lock().unwrap().is_none());
    }

    impl Shared {
        fn is_playing_and_not_ended(&self) -> bool {
            self.playing.load(Ordering::SeqCst) && !self.ended.load(Ordering::SeqCst)
        }
    }

    #[test]
    fn writes_integer_output_formats() {
        let mut frame = [0i16; 2];
        write_frame(&mut frame, 0.5, -1.0);
        assert_eq!(frame, [16_384, -32_768]);
    }
}
