//! The two on-device models, run with `tract` (pure Rust, no native runtime).
//!
//! Both ship inside the app binary, so listening works offline and on a fresh
//! install. Provenance, licenses and the scripts that rebuild them are in
//! `models/README.md`.

use std::sync::Arc;

use mix_core::hearing::Sound;
use tract_onnx::prelude::*;

use crate::mel::MEL_BANDS;
use crate::ListenError;

static SILERO_VAD: &[u8] = include_bytes!("../models/silero_vad.onnx");
static YAMNET: &[u8] = include_bytes!("../models/yamnet.onnx");
static YAMNET_CLASSES: &str = include_str!("../models/yamnet_class_map.csv");

/// Silero takes 512 new samples plus 64 of context at 16 kHz (32 ms).
pub const VAD_CHUNK: usize = 512;
const VAD_CONTEXT: usize = 64;
/// YAMNet looks at 0.96 s of log-mel frames at a time.
pub const PATCH_FRAMES: usize = 96;
pub const CLASS_COUNT: usize = 521;

type Plan = Arc<TypedSimplePlan>;

impl From<TractError> for ListenError {
    fn from(e: TractError) -> Self {
        ListenError::Model(format!("{e:#}"))
    }
}

/// Both models, loaded once and shared by every channel.
#[derive(Clone)]
pub struct Models {
    vad: Plan,
    yamnet: Plan,
    buckets: Arc<Vec<Option<Bucket>>>,
    silence: usize,
}

impl Models {
    /// Parses and optimises both models (about 0.1 s).
    pub fn load() -> Result<Self, ListenError> {
        let vad = tract_onnx::onnx()
            .model_for_read(&mut &SILERO_VAD[..])?
            .with_input_fact(0, f32::fact([1, VAD_CONTEXT + VAD_CHUNK]).into())?
            .with_input_fact(1, f32::fact([2, 1, 128]).into())?
            .with_input_fact(2, InferenceFact::from(tensor0(16_000i64)))?
            .into_optimized()?
            .into_runnable()?;
        let yamnet = tract_onnx::onnx()
            .model_for_read(&mut &YAMNET[..])?
            .with_input_fact(0, f32::fact([1, PATCH_FRAMES, MEL_BANDS]).into())?
            .into_optimized()?
            .into_runnable()?;
        let names = class_names();
        if names.len() != CLASS_COUNT {
            return Err(ListenError::Model(format!(
                "expected {CLASS_COUNT} YAMNet classes, found {}",
                names.len()
            )));
        }
        let silence = names.iter().position(|n| n == "Silence").unwrap_or(0);
        Ok(Self {
            vad,
            yamnet,
            buckets: Arc::new(names.iter().map(|n| bucket(n)).collect()),
            silence,
        })
    }

    /// Runs YAMNet on one patch of `PATCH_FRAMES` log-mel frames (row-major).
    pub fn classify(&self, patch: &[f32]) -> Result<Vec<f32>, ListenError> {
        let input = Tensor::from_shape(&[1, PATCH_FRAMES, MEL_BANDS], patch)?;
        let out = self.yamnet.run(tvec!(input.into()))?;
        Ok(out[0].try_as_plain_ram()?.as_slice::<f32>()?.to_vec())
    }

    /// Boils 521 AudioSet scores down to what matters on a church stage.
    pub fn summarize(&self, scores: &[f32]) -> Summary {
        let mut by_bucket = [0f32; Bucket::COUNT];
        for (score, bucket) in scores.iter().zip(self.buckets.iter()) {
            if let Some(b) = bucket {
                let slot = &mut by_bucket[*b as usize];
                *slot = slot.max(*score);
            }
        }
        let voice = by_bucket[Bucket::Speech as usize]
            .max(by_bucket[Bucket::Singing as usize])
            .max(by_bucket[Bucket::Choir as usize]);
        let silence = scores.get(self.silence).copied().unwrap_or(0.0);

        let specific = Bucket::SPECIFIC
            .iter()
            .map(|&b| (b, by_bucket[b as usize]))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        let (sound, confidence) = match specific {
            Some((b, s)) if s >= SPECIFIC_MIN => (Some(b.sound()), s),
            _ if by_bucket[Bucket::Music as usize] >= GENERAL_MIN => {
                (Some(Sound::Music), by_bucket[Bucket::Music as usize])
            }
            _ if by_bucket[Bucket::Other as usize] >= GENERAL_MIN
                && by_bucket[Bucket::Other as usize] > silence =>
            {
                (Some(Sound::Other), by_bucket[Bucket::Other as usize])
            }
            _ => (None, 0.0),
        };
        Summary {
            sound,
            confidence,
            voice,
        }
    }

    pub(crate) fn vad(&self) -> &Plan {
        &self.vad
    }
}

/// A specific instrument or voice needs this score to be named.
const SPECIFIC_MIN: f32 = 0.15;
/// "Music" or "something else" needs this.
const GENERAL_MIN: f32 = 0.3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Summary {
    pub sound: Option<Sound>,
    pub confidence: f32,
    /// Strongest speech/singing/choir score, 0 to 1.
    pub voice: f32,
}

/// Per-channel Silero state. 32 ms in, one probability out.
pub struct VoiceDetector {
    models: Models,
    state: Tensor,
    context: [f32; VAD_CONTEXT],
}

impl VoiceDetector {
    pub fn new(models: Models) -> Self {
        Self {
            models,
            state: Tensor::zero::<f32>(&[2, 1, 128]).expect("static shape"),
            context: [0.0; VAD_CONTEXT],
        }
    }

    /// Probability that `chunk` (exactly [`VAD_CHUNK`] samples) contains a voice.
    pub fn process(&mut self, chunk: &[f32]) -> Result<f32, ListenError> {
        debug_assert_eq!(chunk.len(), VAD_CHUNK);
        let mut input = Vec::with_capacity(VAD_CONTEXT + VAD_CHUNK);
        input.extend_from_slice(&self.context);
        input.extend_from_slice(chunk);
        self.context
            .copy_from_slice(&chunk[VAD_CHUNK - VAD_CONTEXT..]);
        let input = Tensor::from_shape(&[1, VAD_CONTEXT + VAD_CHUNK], &input)?;
        let out = self.models.vad().run(tvec!(
            input.into(),
            self.state.clone().into(),
            tensor0(16_000i64).into()
        ))?;
        let prob = out[0].try_as_plain_ram()?.as_slice::<f32>()?[0];
        self.state = out[1].clone().into_tensor();
        Ok(prob)
    }
}

fn class_names() -> Vec<String> {
    YAMNET_CLASSES
        .lines()
        .skip(1)
        .filter_map(|line| {
            // index,mid,display_name (the name may be quoted and contain commas)
            let name = line.splitn(3, ',').nth(2)?;
            Some(name.trim().trim_matches('"').to_string())
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bucket {
    Speech,
    Singing,
    Choir,
    Drums,
    Bass,
    ElectricGuitar,
    AcousticGuitar,
    Piano,
    Organ,
    Keys,
    Brass,
    Strings,
    Music,
    Other,
}

impl Bucket {
    const COUNT: usize = 14;
    const SPECIFIC: [Bucket; 12] = [
        Bucket::Speech,
        Bucket::Singing,
        Bucket::Choir,
        Bucket::Drums,
        Bucket::Bass,
        Bucket::ElectricGuitar,
        Bucket::AcousticGuitar,
        Bucket::Piano,
        Bucket::Organ,
        Bucket::Keys,
        Bucket::Brass,
        Bucket::Strings,
    ];

    fn sound(self) -> Sound {
        match self {
            Bucket::Speech => Sound::Speech,
            Bucket::Singing => Sound::Singing,
            Bucket::Choir => Sound::Choir,
            Bucket::Drums => Sound::Drums,
            Bucket::Bass => Sound::Bass,
            Bucket::ElectricGuitar => Sound::ElectricGuitar,
            Bucket::AcousticGuitar => Sound::AcousticGuitar,
            Bucket::Piano => Sound::Piano,
            Bucket::Organ => Sound::Organ,
            Bucket::Keys => Sound::Keys,
            Bucket::Brass => Sound::Brass,
            Bucket::Strings => Sound::Strings,
            Bucket::Music => Sound::Music,
            Bucket::Other => Sound::Other,
        }
    }
}

/// Which bucket an AudioSet class counts towards. Unlisted classes count as
/// "something else", except silence, which counts as nothing.
fn bucket(name: &str) -> Option<Bucket> {
    Some(match name {
        "Speech"
        | "Male speech, man speaking"
        | "Female speech, woman speaking"
        | "Child speech, kid speaking"
        | "Conversation"
        | "Narration, monologue"
        | "Whispering" => Bucket::Speech,
        "Singing" | "Male singing" | "Female singing" | "Child singing" | "Rapping" | "Humming"
        | "Chant" | "Yodeling" | "Vocal music" | "A capella" => Bucket::Singing,
        "Choir" => Bucket::Choir,
        "Drum kit" | "Drum" | "Snare drum" | "Bass drum" | "Drum roll" | "Rimshot" | "Cymbal"
        | "Hi-hat" | "Crash cymbal" | "Percussion" | "Drum machine" | "Tambourine" | "Tabla"
        | "Timpani" => Bucket::Drums,
        "Bass guitar" | "Double bass" => Bucket::Bass,
        "Electric guitar" => Bucket::ElectricGuitar,
        "Acoustic guitar"
        | "Steel guitar, slide guitar"
        | "Strum"
        | "Ukulele"
        | "Banjo"
        | "Mandolin" => Bucket::AcousticGuitar,
        "Piano" | "Electric piano" => Bucket::Piano,
        "Organ" | "Electronic organ" | "Hammond organ" => Bucket::Organ,
        "Synthesizer" | "Sampler" | "Keyboard (musical)" => Bucket::Keys,
        "Brass instrument"
        | "Trumpet"
        | "Trombone"
        | "French horn"
        | "Saxophone"
        | "Wind instrument, woodwind instrument"
        | "Flute"
        | "Clarinet" => Bucket::Brass,
        "Bowed string instrument" | "Violin, fiddle" | "Cello" | "String section" | "Orchestra" => {
            Bucket::Strings
        }
        "Music"
        | "Musical instrument"
        | "Plucked string instrument"
        | "Guitar"
        | "Christian music"
        | "Gospel music" => Bucket::Music,
        "Silence" => return None,
        _ => Bucket::Other,
    })
}
