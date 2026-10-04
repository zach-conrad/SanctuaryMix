"""Writes tests/golden.json: reference numbers the Rust code must reproduce.

Run from this folder with the YAMNet sources next to it (see export_yamnet.py)
and onnxruntime installed. The test signal is generated the same way in
tests/golden.rs.
"""
import json
import numpy as np
import onnxruntime as ort
import tensorflow as tf
import params as P
import features as F

p = P.Params()
n = np.arange(32000, dtype=np.float64)
sr = 16000.0
x = (0.3 * np.sin(2 * np.pi * 220 * n / sr)
     + 0.2 * np.sin(2 * np.pi * 1250 * n / sr)
     + 0.1 * np.sin(2 * np.pi * (300 + 2000 * n / 32000) * n / sr)).astype(np.float32)

log_mel, patches = F.waveform_to_log_mel_spectrogram_patches(tf.constant(x), p)
log_mel = log_mel.numpy()
yam = ort.InferenceSession("../yamnet.onnx", providers=["CPUExecutionProvider"])
scores = yam.run(None, {"patches": patches.numpy()[:1]})[0][0]
top = np.argsort(-scores)[:5]

vad = ort.InferenceSession("../silero_vad.onnx", providers=["CPUExecutionProvider"])
state = np.zeros((2, 1, 128), np.float32)
context = np.zeros(64, np.float32)
probs = []
for i in range(20):
    chunk = x[i * 512:(i + 1) * 512]
    inp = np.concatenate([context, chunk])[None, :]
    out, state = vad.run(None, {"input": inp, "state": state, "sr": np.array(16000, np.int64)})
    context = chunk[-64:]
    probs.append(float(out.reshape(-1)[0]))

frames = [0, 1, 40, 97, 195]
json.dump({
    "log_mel_frames": {str(f): [round(float(v), 4) for v in log_mel[f]] for f in frames},
    "log_mel_frame_count": int(log_mel.shape[0]),
    "yamnet_top": [[int(i), round(float(scores[i]), 5)] for i in top],
    "vad_probs": [round(v, 6) for v in probs],
}, open("../../tests/golden.json", "w"), indent=1)
print("top", top, scores[top], "vad", probs[:5])
