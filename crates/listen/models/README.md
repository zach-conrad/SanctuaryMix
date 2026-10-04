# Listening models

Both models are compiled into the app (`include_bytes!` in `src/models.rs`), so
listening works offline and costs nothing to run.

| File | What it is | Source | License |
| --- | --- | --- | --- |
| `silero_vad.onnx` | Silero VAD v5, 16 kHz only. Says every 32 ms whether a voice is present. | `src/silero_vad/data/silero_vad_16k_op15.onnx` from [snakers4/silero-vad](https://github.com/snakers4/silero-vad) | MIT |
| `yamnet.onnx` | YAMNet sound classifier (521 AudioSet classes), from log-mel patches to scores. | [`yamnet.h5`](https://storage.googleapis.com/audioset/yamnet.h5) from [tensorflow/models research/audioset/yamnet](https://github.com/tensorflow/models/tree/master/research/audioset/yamnet) | Apache-2.0 |
| `yamnet_class_map.csv` | YAMNet's class names | same | Apache-2.0 |

## How they were made

- `tools/inline_silero.py` takes Silero's 16 kHz ONNX file and inlines its
  `If` branches for a fixed 16 kHz, batch-of-one input. `tract` can't type
  those branches (one returns a squeezed shape), and nothing else changes: the
  script checks the output matches the original with onnxruntime.
- `tools/export_yamnet.py` rebuilds YAMNet's network from Google's code and
  weights and exports only the part after feature extraction. The log-mel
  features are computed in Rust (`src/mel.rs`) to match Google's
  `features.py`.
- `tools/make_golden.py` writes `tests/golden.json`. `tests/golden.rs` checks
  the Rust features and both models against it, so a model swap or a `tract`
  upgrade that changes results fails CI.

## Trying them on a recording

```sh
cargo run --release -p listen --example hear -- service.wav 3
```

prints, once a second, whether channel 3 of the WAV file has a voice on it and
what it sounds like.
