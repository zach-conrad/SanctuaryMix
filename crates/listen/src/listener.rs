//! Per-channel listening on its own thread.
//!
//! The audio thread hands over raw interleaved blocks; this thread picks out
//! the channels it was asked to listen to, converts them to 16 kHz and runs
//! the models. Results go out about ten times a second as a
//! [`HearingFrame`]. If the models ever fall behind, blocks are dropped rather
//! than queued, so listening can never delay metering or the console.

use std::collections::{BTreeMap, VecDeque};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use mix_core::hearing::{Hearing, HearingFrame, Sound};
use serde::Serialize;

use crate::mel::{LogMel, MEL_BANDS};
use crate::models::{Models, Summary, VoiceDetector, PATCH_FRAMES, VAD_CHUNK};
use crate::resample::{Resampler, MODEL_RATE};

/// How often hearing results are published.
const PUBLISH_EVERY: Duration = Duration::from_millis(100);
/// Silero probability that counts as a voice.
const VAD_ON: f32 = 0.5;
/// Below this it's definitely not (hysteresis).
const VAD_OFF: f32 = 0.35;
/// Keep "voice" on through the gaps between words and phrases.
const VOICE_HANG_SAMPLES: u64 = MODEL_RATE as u64 * 2 / 5;
/// A YAMNet speech/singing score this strong also counts as a voice...
const CLASSIFIER_VOICE_MIN: f32 = 0.3;
/// ...until the next patch or this long after it.
const CLASSIFIER_HOLD_SAMPLES: u64 = MODEL_RATE as u64 * 6 / 5;
/// Patches quieter than this (dBFS RMS) aren't classified: nothing to hear.
const QUIET_DBFS: f32 = -60.0;
/// Room for this many audio blocks between the audio side and this thread.
const QUEUE_BLOCKS: usize = 8;

/// One input to listen to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListenTarget {
    pub channel: u16,
    /// Run the voice detector too (speech and vocal mics).
    pub voice: bool,
}

/// What an input has mostly sounded like since listening started.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeardSummary {
    pub channel: u16,
    pub sound: Sound,
    /// Share of what was heard that was `sound`, 0 to 1.
    pub share: f32,
    /// How much audio with something in it this is based on.
    pub seconds: f32,
}

enum Msg {
    Audio {
        channels: u16,
        rate: u32,
        samples: Vec<f32>,
    },
    Targets(Vec<ListenTarget>),
    Scan(Duration),
    Stop,
}

/// Handle to the listening thread. Dropping it stops the thread.
pub struct Listener {
    tx: SyncSender<Msg>,
    heard: Arc<Mutex<BTreeMap<u16, Tally>>>,
    thread: Option<JoinHandle<()>>,
}

impl Listener {
    pub fn start(models: Models, on_hearing: impl FnMut(HearingFrame) + Send + 'static) -> Self {
        let (tx, rx) = mpsc::sync_channel(QUEUE_BLOCKS);
        let heard = Arc::new(Mutex::new(BTreeMap::new()));
        let worker = Worker {
            models,
            ears: BTreeMap::new(),
            targets: Vec::new(),
            scan_until: None,
            rate: 0,
            heard: heard.clone(),
            scratch: Vec::new(),
        };
        let thread = std::thread::Builder::new()
            .name("sanctuarymix-listen".into())
            .spawn(move || worker.run(rx, on_hearing))
            .expect("spawn listening thread");
        Self {
            tx,
            heard,
            thread: Some(thread),
        }
    }

    /// Hands over one interleaved block. Never blocks; returns false if the
    /// listening thread is behind and the block was skipped.
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

    /// The inputs to keep listening to (auto-mix's channels).
    pub fn set_targets(&self, targets: Vec<ListenTarget>) {
        let _ = self.tx.send(Msg::Targets(targets));
    }

    /// Listens to every input with signal for `secs`, to suggest roles.
    pub fn scan(&self, duration: Duration) {
        let _ = self.tx.send(Msg::Scan(duration));
    }

    /// What each input has mostly sounded like.
    pub fn heard(&self) -> Vec<HeardSummary> {
        let heard = self.heard.lock().unwrap_or_else(|p| p.into_inner());
        heard
            .iter()
            .filter_map(|(&channel, tally)| tally.summary(channel))
            .collect()
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Stop);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Time-weighted count of what an input sounded like.
#[derive(Debug, Clone, Default)]
struct Tally {
    by_sound: BTreeMap<Sound, f32>,
    patches: u32,
}

impl Tally {
    fn add(&mut self, summary: &Summary) {
        if let Some(sound) = summary.sound {
            *self.by_sound.entry(sound).or_default() += summary.confidence;
            self.patches += 1;
        }
    }

    fn summary(&self, channel: u16) -> Option<HeardSummary> {
        let total: f32 = self.by_sound.values().sum();
        let (&sound, &weight) = self
            .by_sound
            .iter()
            .filter(|(s, _)| **s != Sound::Other)
            .max_by(|a, b| a.1.total_cmp(b.1))?;
        Some(HeardSummary {
            channel,
            sound,
            share: weight / total.max(1e-6),
            seconds: self.patches as f32 * PATCH_FRAMES as f32 / 100.0,
        })
    }
}

struct Worker {
    models: Models,
    ears: BTreeMap<u16, Ear>,
    targets: Vec<ListenTarget>,
    scan_until: Option<Instant>,
    rate: u32,
    heard: Arc<Mutex<BTreeMap<u16, Tally>>>,
    scratch: Vec<f32>,
}

impl Worker {
    fn run(mut self, rx: Receiver<Msg>, mut on_hearing: impl FnMut(HearingFrame)) {
        let mut last_publish = Instant::now();
        let mut failed = false;
        loop {
            match rx.recv_timeout(PUBLISH_EVERY) {
                Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                Ok(Msg::Targets(targets)) => {
                    self.targets = targets;
                    self.prune();
                }
                Ok(Msg::Scan(duration)) => {
                    self.scan_until = Some(Instant::now() + duration);
                    self.heard.lock().unwrap_or_else(|p| p.into_inner()).clear();
                }
                Ok(Msg::Audio {
                    channels,
                    rate,
                    samples,
                }) => {
                    if let Err(e) = self.on_audio(channels, rate, &samples) {
                        if !failed {
                            log::error!("listening stopped working: {e}");
                            failed = true;
                        }
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
            if self.scan_until.is_some_and(|until| Instant::now() >= until) {
                self.scan_until = None;
                self.prune();
            }
            if last_publish.elapsed() >= PUBLISH_EVERY {
                last_publish = Instant::now();
                let frame = HearingFrame {
                    channels: self
                        .targets
                        .iter()
                        .filter_map(|t| self.ears.get(&t.channel).map(|e| e.hearing(t.channel)))
                        .collect(),
                };
                if !frame.channels.is_empty() {
                    on_hearing(frame);
                }
            }
        }
    }

    /// Drops the ears nobody needs any more.
    fn prune(&mut self) {
        let scanning = self.scan_until.is_some();
        let targets = &self.targets;
        self.ears.retain(|ch, ear| {
            let target = targets.iter().find(|t| t.channel == *ch);
            match target {
                Some(t) => ear.vad.is_some() == t.voice,
                None => scanning,
            }
        });
    }

    fn on_audio(
        &mut self,
        channels: u16,
        rate: u32,
        samples: &[f32],
    ) -> Result<(), crate::ListenError> {
        if rate != self.rate {
            self.rate = rate;
            self.ears.clear();
        }
        let n = channels as usize;
        if n == 0 {
            return Ok(());
        }
        let frames = samples.len() / n;
        let scanning = self.scan_until.is_some();
        for ch in 0..channels {
            let target = self.targets.iter().find(|t| t.channel == ch).copied();
            if target.is_none() && !scanning {
                continue;
            }
            self.scratch.clear();
            self.scratch
                .extend((0..frames).map(|f| samples[f * n + ch as usize]));
            let models = &self.models;
            let ear = self
                .ears
                .entry(ch)
                .or_insert_with(|| Ear::new(models, rate, target.is_some_and(|t| t.voice)));
            if let Some(summary) = ear.process(&self.scratch)? {
                self.heard
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .entry(ch)
                    .or_default()
                    .add(&summary);
            }
        }
        Ok(())
    }
}

/// Listening state for one input.
struct Ear {
    models: Models,
    resampler: Resampler,
    mel: LogMel,
    frames: VecDeque<[f32; MEL_BANDS]>,
    new_frames: usize,
    vad: Option<VoiceDetector>,
    vad_pending: Vec<f32>,
    /// 16 kHz samples seen so far: this ear's clock.
    clock: u64,
    patch_energy: f64,
    patch_samples: u64,
    vad_prob: f32,
    vad_voice: bool,
    last_vad_voice: Option<u64>,
    last_summary: Option<(u64, Summary)>,
    out: Vec<f32>,
}

impl Ear {
    fn new(models: &Models, rate: u32, voice: bool) -> Self {
        Self {
            models: models.clone(),
            resampler: Resampler::new(rate),
            mel: LogMel::new(),
            frames: VecDeque::with_capacity(PATCH_FRAMES),
            new_frames: 0,
            vad: voice.then(|| VoiceDetector::new(models.clone())),
            vad_pending: Vec::with_capacity(VAD_CHUNK * 2),
            clock: 0,
            patch_energy: 0.0,
            patch_samples: 0,
            vad_prob: 0.0,
            vad_voice: false,
            last_vad_voice: None,
            last_summary: None,
            out: Vec::new(),
        }
    }

    /// Feeds audio at the device rate. Returns a classification when a new patch completes.
    fn process(&mut self, input: &[f32]) -> Result<Option<Summary>, crate::ListenError> {
        self.out.clear();
        self.resampler.process(input, &mut self.out);
        let audio = std::mem::take(&mut self.out);
        self.patch_energy += audio.iter().map(|s| (*s as f64) * (*s as f64)).sum::<f64>();
        self.patch_samples += audio.len() as u64;

        if let Some(vad) = self.vad.as_mut() {
            self.vad_pending.extend_from_slice(&audio);
            let mut start = 0;
            while start + VAD_CHUNK <= self.vad_pending.len() {
                let chunk = &self.vad_pending[start..start + VAD_CHUNK];
                let rms = (chunk.iter().map(|s| s * s).sum::<f32>() / VAD_CHUNK as f32).sqrt();
                // Silent chunks still go through so the model's memory stays in step.
                let prob = vad.process(chunk)?;
                self.vad_prob = if rms > 1e-5 { prob } else { 0.0 };
                let at = self.clock + (start + VAD_CHUNK) as u64;
                if self.vad_prob >= VAD_ON || (self.vad_voice && self.vad_prob > VAD_OFF) {
                    self.vad_voice = true;
                    self.last_vad_voice = Some(at);
                } else {
                    self.vad_voice = false;
                }
                start += VAD_CHUNK;
            }
            self.vad_pending.drain(..start);
        }

        let mut finished = None;
        let frames = &mut self.frames;
        let new_frames = &mut self.new_frames;
        self.mel.process(&audio, |frame| {
            if frames.len() == PATCH_FRAMES {
                frames.pop_front();
            }
            frames.push_back(*frame);
            *new_frames += 1;
        });
        self.clock += audio.len() as u64;
        self.out = audio;

        // A fresh, non-overlapping patch every 0.96 s.
        if self.frames.len() == PATCH_FRAMES && self.new_frames >= PATCH_FRAMES {
            self.new_frames = 0;
            let rms = (self.patch_energy / self.patch_samples.max(1) as f64).sqrt() as f32;
            self.patch_energy = 0.0;
            self.patch_samples = 0;
            let summary = if 20.0 * rms.max(1e-9).log10() < QUIET_DBFS {
                Summary {
                    sound: None,
                    confidence: 0.0,
                    voice: 0.0,
                }
            } else {
                let patch: Vec<f32> = self.frames.iter().flatten().copied().collect();
                let scores = self.models.classify(&patch)?;
                self.models.summarize(&scores)
            };
            self.last_summary = Some((self.clock, summary));
            finished = Some(summary);
        }
        Ok(finished)
    }

    fn hearing(&self, channel: u16) -> Hearing {
        let vad_voice = self
            .last_vad_voice
            .is_some_and(|at| self.clock.saturating_sub(at) <= VOICE_HANG_SAMPLES);
        let recent = self
            .last_summary
            .filter(|(at, _)| self.clock.saturating_sub(*at) <= CLASSIFIER_HOLD_SAMPLES)
            .map(|(_, s)| s);
        let classifier_voice = recent.is_some_and(|s| s.voice >= CLASSIFIER_VOICE_MIN);
        let latest = self.last_summary.map(|(_, s)| s);
        Hearing {
            channel,
            voice: vad_voice || classifier_voice,
            voice_prob: if self.vad.is_some() {
                self.vad_prob
            } else {
                recent.map_or(0.0, |s| s.voice)
            },
            sound: latest.and_then(|s| s.sound),
            confidence: latest.map_or(0.0, |s| s.confidence),
        }
    }
}
