"""Rebuilds models/yamnet.onnx from Google's published YAMNet weights.

The ONNX model takes log-mel patches [batch, 96, 64] and returns the 521
AudioSet class scores. Feature extraction is done in Rust (src/mel.rs).

    pip install tensorflow-cpu tf_keras tf2onnx
    curl -O https://storage.googleapis.com/audioset/yamnet.h5
    for f in yamnet.py params.py features.py; do
      curl -O https://raw.githubusercontent.com/tensorflow/models/master/research/audioset/yamnet/$f
    done
    python3 export_yamnet.py   # writes yamnet.onnx
"""
import tensorflow as tf
import tf2onnx
from tf_keras import layers, Model

import params as P
import yamnet as Y

p = P.Params()
full = Y.yamnet_frames_model(p)
full.load_weights("yamnet.h5")

inp = layers.Input(shape=(p.patch_frames, p.patch_bands), dtype=tf.float32, name="patches")
pred, _ = Y.yamnet(inp, p)
patches_model = Model(inputs=inp, outputs=[pred])
assert len(patches_model.weights) == len(full.weights)
for w, f in zip(patches_model.weights, full.weights):
    assert w.shape == f.shape, (w.name, f.name)
    w.assign(f)

spec = (tf.TensorSpec((None, p.patch_frames, p.patch_bands), tf.float32, name="patches"),)
tf2onnx.convert.from_keras(patches_model, input_signature=spec, opset=13, output_path="yamnet.onnx")
