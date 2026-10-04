//! Checks the Rust feature code and both models against reference numbers
//! from Google's and Silero's own Python code (`models/tools/make_golden.py`).

use listen::mel::{LogMel, MEL_BANDS};
use listen::models::{Models, VoiceDetector, CLASS_COUNT, PATCH_FRAMES, VAD_CHUNK};

/// The same 2 s test signal as make_golden.py.
fn signal() -> Vec<f32> {
    let sr = 16_000.0f64;
    (0..32_000)
        .map(|n| {
            let n = n as f64;
            let tau = 2.0 * std::f64::consts::PI;
            (0.3 * (tau * 220.0 * n / sr).sin()
                + 0.2 * (tau * 1250.0 * n / sr).sin()
                + 0.1 * (tau * (300.0 + 2000.0 * n / 32000.0) * n / sr).sin()) as f32
        })
        .collect()
}

fn golden() -> serde_json::Value {
    serde_json::from_str(include_str!("golden.json")).unwrap()
}

fn log_mel(x: &[f32]) -> Vec<[f32; MEL_BANDS]> {
    let mut mel = LogMel::new();
    let mut frames = Vec::new();
    // Uneven blocks, like real audio callbacks.
    for chunk in x.chunks(333) {
        mel.process(chunk, |f| frames.push(*f));
    }
    frames
}

#[test]
fn log_mel_matches_tensorflow() {
    let g = golden();
    let frames = log_mel(&signal());
    assert_eq!(
        frames.len() as u64,
        g["log_mel_frame_count"].as_u64().unwrap()
    );
    for (index, expected) in g["log_mel_frames"].as_object().unwrap() {
        let got = &frames[index.parse::<usize>().unwrap()];
        for (band, e) in expected.as_array().unwrap().iter().enumerate() {
            let e = e.as_f64().unwrap() as f32;
            assert!(
                (got[band] - e).abs() < 2e-3,
                "frame {index} band {band}: {} vs {e}",
                got[band]
            );
        }
    }
}

#[test]
fn yamnet_matches_reference() {
    let models = Models::load().unwrap();
    let frames = log_mel(&signal());
    let patch: Vec<f32> = frames[..PATCH_FRAMES].iter().flatten().copied().collect();
    let scores = models.classify(&patch).unwrap();
    assert_eq!(scores.len(), CLASS_COUNT);
    for pair in golden()["yamnet_top"].as_array().unwrap() {
        let class = pair[0].as_u64().unwrap() as usize;
        let expected = pair[1].as_f64().unwrap() as f32;
        assert!(
            (scores[class] - expected).abs() < 1e-3,
            "class {class}: {} vs {expected}",
            scores[class]
        );
    }
}

#[test]
fn voice_detector_matches_reference() {
    let models = Models::load().unwrap();
    let mut vad = VoiceDetector::new(models);
    let x = signal();
    for (i, expected) in golden()["vad_probs"].as_array().unwrap().iter().enumerate() {
        let p = vad.process(&x[i * VAD_CHUNK..(i + 1) * VAD_CHUNK]).unwrap();
        let expected = expected.as_f64().unwrap() as f32;
        assert!((p - expected).abs() < 1e-3, "chunk {i}: {p} vs {expected}");
    }
}
