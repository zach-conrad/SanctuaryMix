//! Hearing each picked channel's tone, on its own thread.
//!
//! The audio thread hands over raw interleaved blocks (the same hand-off the
//! listening models use). For each picked channel this thread runs a
//! 4096-point Hann STFT (85 ms at 48 kHz) and:
//!
//! - folds every frame that passes the gates into a long-term average
//!   spectrum (LTAS) in 1/6-octave bands, an exponential average over about
//!   30 seconds of *gated* audio. The level gate skips quiet frames; on voice
//!   mics the source gate counts only frames where the voice detector hears
//!   someone at that mic, so the pastor is measured, not the band in the lav;
//! - runs the [`FeedbackDetector`] on every frame, gated or not.
//!
//! If it falls behind, blocks are dropped rather than queued, so analysis can
//! never delay metering, listening or the console.

use std::collections::BTreeMap;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};

use crate::feedback::{FeedbackDetector, Howl};

pub const FFT_LEN: usize = 4096;
const HOP: usize = 1024;
/// 1/6-octave bands from 20 Hz: 60 of them reach about 20 kHz.
pub const SPECTRUM_BANDS: usize = 60;
pub const SPECTRUM_LOW_HZ: f32 = 20.0;
/// Gated seconds the long-term average spans.
const LTAS_SECONDS: f32 = 30.0;
/// Frames quieter than this (dBFS RMS) don't count.
const LEVEL_GATE_DBFS: f32 = -60.0;
/// ...nor frames this far under the channel's running peak level.
const BELOW_PEAK_GATE_DB: f32 = 30.0;
/// The source gate trusts a voice result this long.
const VOICE_STALE: Duration = Duration::from_millis(1500);
const PUBLISH_EVERY: Duration = Duration::from_millis(250);
const QUEUE_BLOCKS: usize = 8;

/// Centre frequency of spectrum band `i`.
pub fn band_hz(i: usize) -> f32 {
    SPECTRUM_LOW_HZ * (i as f32 / 6.0).exp2()
}

/// The band a frequency falls in (nearest centre), if inside the range.
pub fn band_of(hz: f32) -> Option<usize> {
    if !(hz.is_finite() && hz > 0.0) {
        return None;
    }
    let i = (6.0 * (hz / SPECTRUM_LOW_HZ).log2()).round();
    (0.0..SPECTRUM_BANDS as f32)
        .contains(&i)
        .then_some(i as usize)
}

/// One channel to analyse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalyseTarget {
    pub channel: u16,
    /// Count only frames where the voice detector hears someone at this mic.
    pub voice_gate: bool,
    /// Run the feedback detector (every picked channel).
    pub feedback: bool,
}

/// What one channel sounds like so far.
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelTone {
    pub channel: u16,
    /// LTAS in dB per 1/6-octave band (absolute, before the desk's EQ when
    /// tapped before it). `None` until anything passed the gates.
    pub ltas_db: Option<Vec<f32>>,
    /// Seconds of gated audio since the last reset.
    pub gated_secs: f32,
    /// Seconds of gated audio in the last publish period: the channel is in use.
    pub active: bool,
}

/// Published about four times a second, and straight away on a howl.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ToneFrame {
    pub channels: Vec<ChannelTone>,
    pub howls: Vec<Howl>,
}

enum Msg {
    Audio {
        channels: u16,
        rate: u32,
        samples: Vec<f32>,
    },
    Targets(Vec<AnalyseTarget>),
    Voice(Vec<(u16, bool)>),
    Reset(Vec<u16>),
    Stop,
}

/// Handle to the analysis thread. Dropping it stops the thread.
pub struct Analyser {
    tx: SyncSender<Msg>,
    thread: Option<JoinHandle<()>>,
}

impl Analyser {
    pub fn start(on_frame: impl FnMut(ToneFrame) + Send + 'static) -> Self {
        let (tx, rx) = mpsc::sync_channel(QUEUE_BLOCKS);
        let thread = std::thread::Builder::new()
            .name("sanctuarymix-tonal".into())
            .spawn(move || Worker::new().run(rx, on_frame))
            .expect("spawn tonal thread");
        Self {
            tx,
            thread: Some(thread),
        }
    }

    /// Hands over one interleaved block. Never blocks; false if it was skipped.
    pub fn push_audio(&self, channels: u16, rate: u32, samples: &[f32]) -> bool {
        match self.tx.try_send(Msg::Audio {
            channels,
            rate,
            samples: samples.to_vec(),
        }) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => false,
        }
    }

    pub fn set_targets(&self, targets: Vec<AnalyseTarget>) {
        let _ = self.tx.send(Msg::Targets(targets));
    }

    /// Latest voice results (channel, someone at the mic). Never blocks.
    pub fn push_voice(&self, voice: Vec<(u16, bool)>) {
        let _ = self.tx.try_send(Msg::Voice(voice));
    }

    /// Starts these channels' averages over (a new soundcheck).
    pub fn reset(&self, channels: Vec<u16>) {
        let _ = self.tx.send(Msg::Reset(channels));
    }
}

impl Drop for Analyser {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Stop);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Per-channel state.
struct Ear {
    target: AnalyseTarget,
    buf: Vec<f32>,
    ltas_pow: Option<Vec<f32>>,
    gated_secs: f32,
    gated_since_publish: f32,
    peak_db: f32,
    voice: Option<(Instant, bool)>,
    feedback: FeedbackDetector,
}

impl Ear {
    fn new(target: AnalyseTarget) -> Self {
        Self {
            target,
            buf: Vec::with_capacity(FFT_LEN * 2),
            ltas_pow: None,
            gated_secs: 0.0,
            gated_since_publish: 0.0,
            peak_db: -120.0,
            voice: None,
            feedback: FeedbackDetector::new(),
        }
    }

    fn voice_ok(&self, now: Instant) -> bool {
        if !self.target.voice_gate {
            return true;
        }
        // Without listening results the level gate alone decides.
        match self.voice {
            Some((at, v)) if now.duration_since(at) < VOICE_STALE => v,
            _ => true,
        }
    }
}

struct Worker {
    ears: BTreeMap<u16, Ear>,
    fft: Option<(u32, Arc<dyn Fft<f32>>)>,
    window: Vec<f32>,
    scratch: Vec<Complex32>,
    /// FFT bin range of each spectrum band, for the current rate.
    band_bins: Vec<(usize, usize)>,
    howls: Vec<Howl>,
}

impl Worker {
    fn new() -> Self {
        let window = (0..FFT_LEN)
            .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / FFT_LEN as f32).cos())
            .collect();
        Self {
            ears: BTreeMap::new(),
            fft: None,
            window,
            scratch: vec![Complex32::default(); FFT_LEN],
            band_bins: Vec::new(),
            howls: Vec::new(),
        }
    }

    fn run(mut self, rx: Receiver<Msg>, mut on_frame: impl FnMut(ToneFrame)) {
        let mut last_publish = Instant::now();
        loop {
            match rx.recv_timeout(PUBLISH_EVERY) {
                Ok(Msg::Audio {
                    channels,
                    rate,
                    samples,
                }) => self.audio(channels, rate, &samples),
                Ok(Msg::Targets(targets)) => {
                    let mut ears = std::mem::take(&mut self.ears);
                    for t in targets {
                        let mut ear = ears.remove(&t.channel).unwrap_or_else(|| Ear::new(t));
                        ear.target = t;
                        self.ears.insert(t.channel, ear);
                    }
                }
                Ok(Msg::Voice(voice)) => {
                    let now = Instant::now();
                    for (ch, v) in voice {
                        if let Some(ear) = self.ears.get_mut(&ch) {
                            ear.voice = Some((now, v));
                        }
                    }
                }
                Ok(Msg::Reset(channels)) => {
                    for ch in channels {
                        if let Some(ear) = self.ears.get_mut(&ch) {
                            ear.ltas_pow = None;
                            ear.gated_secs = 0.0;
                        }
                    }
                }
                Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {}
            }
            let howl_waiting = !self.howls.is_empty();
            if howl_waiting || last_publish.elapsed() >= PUBLISH_EVERY {
                on_frame(self.frame());
                last_publish = Instant::now();
            }
        }
    }

    fn frame(&mut self) -> ToneFrame {
        let channels = self
            .ears
            .values_mut()
            .map(|ear| {
                let active = ear.gated_since_publish > 0.05;
                ear.gated_since_publish = 0.0;
                ChannelTone {
                    channel: ear.target.channel,
                    ltas_db: ear
                        .ltas_pow
                        .as_ref()
                        .map(|p| p.iter().map(|&x| 10.0 * x.max(1e-14).log10()).collect()),
                    gated_secs: ear.gated_secs,
                    active,
                }
            })
            .collect();
        ToneFrame {
            channels,
            howls: std::mem::take(&mut self.howls),
        }
    }

    fn set_rate(&mut self, rate: u32) {
        if self.fft.as_ref().is_some_and(|(r, _)| *r == rate) {
            return;
        }
        let fft = FftPlanner::new().plan_fft_forward(FFT_LEN);
        self.fft = Some((rate, fft));
        let bin_hz = rate as f32 / FFT_LEN as f32;
        let nyquist_bin = FFT_LEN / 2;
        self.band_bins = (0..SPECTRUM_BANDS)
            .map(|i| {
                let lo = band_hz(i) * (-1.0f32 / 12.0).exp2();
                let hi = band_hz(i) * (1.0f32 / 12.0).exp2();
                let a = ((lo / bin_hz).ceil() as usize).clamp(1, nyquist_bin);
                let b = ((hi / bin_hz).floor() as usize).clamp(a, nyquist_bin);
                (a, b)
            })
            .collect();
        for ear in self.ears.values_mut() {
            ear.buf.clear();
            ear.feedback = FeedbackDetector::new();
        }
    }

    fn audio(&mut self, channels: u16, rate: u32, samples: &[f32]) {
        if channels == 0 || rate == 0 {
            return;
        }
        self.set_rate(rate);
        let n = channels as usize;
        let chans: Vec<u16> = self.ears.keys().copied().collect();
        for ch in chans {
            if ch as usize >= n {
                continue;
            }
            let ear = self.ears.get_mut(&ch).unwrap();
            ear.buf
                .extend(samples.iter().skip(ch as usize).step_by(n).copied());
            while self.ears[&ch].buf.len() >= FFT_LEN {
                self.analyse_frame(ch, rate);
                self.ears.get_mut(&ch).unwrap().buf.drain(..HOP);
            }
        }
    }

    fn analyse_frame(&mut self, ch: u16, rate: u32) {
        let now = Instant::now();
        let Some((_, fft)) = self.fft.clone() else {
            return;
        };
        let ear = self.ears.get_mut(&ch).unwrap();
        let mut sum_sq = 0.0f32;
        for (i, (&x, w)) in ear.buf[..FFT_LEN].iter().zip(&self.window).enumerate() {
            sum_sq += x * x;
            self.scratch[i] = Complex32::new(x * w, 0.0);
        }
        let rms_db = 10.0 * (sum_sq / FFT_LEN as f32).max(1e-14).log10();
        fft.process(&mut self.scratch);
        // Power spectrum, normalised so a full-scale sine reads about 0 dB.
        let norm = 4.0 / (FFT_LEN as f32 * FFT_LEN as f32 / 4.0);
        let power: Vec<f32> = self.scratch[..=FFT_LEN / 2]
            .iter()
            .map(|c| c.norm_sqr() * norm)
            .collect();

        let hop_secs = HOP as f32 / rate as f32;
        if ear.target.feedback {
            if let Some(howl) = ear.feedback.process(&power, rate, hop_secs, rms_db, ch) {
                self.howls.push(howl);
            }
        }

        // Running peak level, for the relative gate.
        ear.peak_db = (ear.peak_db - 3.0 * hop_secs).max(rms_db);
        let loud_enough = rms_db > LEVEL_GATE_DBFS && rms_db > ear.peak_db - BELOW_PEAK_GATE_DB;
        if !(loud_enough && ear.voice_ok(now)) {
            return;
        }
        let bands: Vec<f32> = self
            .band_bins
            .iter()
            .map(|&(a, b)| {
                if b < a || b >= power.len() {
                    return 1e-14;
                }
                // Band energy (pink noise reads flat), like a 1/n-octave analyser.
                power[a..=b].iter().sum::<f32>()
            })
            .collect();
        let alpha = (hop_secs / LTAS_SECONDS).min(1.0);
        match ear.ltas_pow.as_mut() {
            None => ear.ltas_pow = Some(bands),
            Some(acc) => {
                // Faster at first, so a short soundcheck already counts fully.
                let a = alpha.max(hop_secs / (ear.gated_secs + hop_secs));
                for (x, b) in acc.iter_mut().zip(bands) {
                    *x += a * (b - *x);
                }
            }
        }
        ear.gated_secs += hop_secs;
        ear.gated_since_publish += hop_secs;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bands_are_sixth_octaves_from_20_hz() {
        assert!((band_hz(0) - 20.0).abs() < 1e-3);
        assert!((band_hz(6) - 40.0).abs() < 1e-3);
        assert!(band_hz(SPECTRUM_BANDS - 1) > 18_000.0);
        assert_eq!(band_of(1_000.0), Some(34));
        assert_eq!(band_of(5.0), None);
    }

    #[test]
    fn hears_where_the_energy_is() {
        let mut w = Worker::new();
        w.ears.insert(
            0,
            Ear::new(AnalyseTarget {
                channel: 0,
                voice_gate: false,
                feedback: false,
            }),
        );
        // Two channels interleaved; channel 0 carries a 1 kHz tone plus a
        // quieter 200 Hz one.
        let rate = 48_000;
        let mut samples = Vec::new();
        for i in 0..rate {
            let t = i as f32 / rate as f32;
            let x = 0.3 * (2.0 * std::f32::consts::PI * 1_000.0 * t).sin()
                + 0.03 * (2.0 * std::f32::consts::PI * 200.0 * t).sin();
            samples.extend([x, 0.0]);
        }
        w.audio(2, rate, &samples);
        let frame = w.frame();
        let tone = &frame.channels[0];
        assert!(tone.gated_secs > 0.8, "{}", tone.gated_secs);
        let ltas = tone.ltas_db.as_ref().unwrap();
        let k1 = band_of(1_000.0).unwrap();
        let k2 = band_of(200.0).unwrap();
        let k5 = band_of(5_000.0).unwrap();
        assert!(
            (ltas[k1] - ltas[k2] - 20.0).abs() < 2.0,
            "{} vs {}",
            ltas[k1],
            ltas[k2]
        );
        assert!(ltas[k2] > ltas[k5] + 20.0);
    }

    #[test]
    fn quiet_frames_dont_count() {
        let mut w = Worker::new();
        w.ears.insert(
            0,
            Ear::new(AnalyseTarget {
                channel: 0,
                voice_gate: false,
                feedback: false,
            }),
        );
        let samples = vec![1e-5f32; 48_000];
        w.audio(1, 48_000, &samples);
        assert_eq!(w.frame().channels[0].gated_secs, 0.0);
    }
}
