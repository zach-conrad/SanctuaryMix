#!/usr/bin/env python3
"""Builds a synthetic service recording bundle for testing the Services screen.

The bundle has the exact on-disk layout the app writes (docs/RECORDINGS.md):
manifest.json, events.jsonl, mix.wav and tracks/in-NN.wav. Copy the folder
into the app's recordings folder and relaunch; the app imports it.

The audio is synthetic (tones, noise and simple instruments), and the stereo
mix is rendered from the tracks with every recorded fader move and mute
applied, so what you hear matches the timeline.

Needs numpy:  python3 -m pip install numpy
Usage:        python3 scripts/make-sample-service.py OUT_DIR
"""

import json
import sys
import uuid
import wave
from datetime import datetime, timedelta, timezone
from pathlib import Path

import numpy as np

SR = 48_000
LENGTH_S = 150
BPM = 76
N = SR * LENGTH_S
rng = np.random.default_rng(7)
t = np.arange(N) / SR

# (name, pan -1..1, starting fader dB)
CHANNELS = [
    ("Pastor", 0.0, -4.0),
    ("Worship Ld", 0.0, -3.0),
    ("BGV 1", -0.3, -8.0),
    ("BGV 2", 0.3, -8.0),
    ("Kick", 0.0, -6.0),
    ("Snare", 0.0, -7.0),
    ("OH L", -0.8, -14.0),
    ("OH R", 0.8, -14.0),
    ("Bass DI", 0.0, -7.0),
    ("Elec Gtr", 0.4, -12.0),
    ("Acous Gtr", -0.4, -9.0),
    ("Keys L", -0.9, -11.0),
    ("Keys R", 0.9, -11.0),
    ("Playback L", -1.0, -10.0),
    ("Playback R", 1.0, -10.0),
    ("Handheld 1", 0.0, -6.0),
]
MIX_INPUTS = [16, 17]  # Dante inputs 17 and 18 carry Main L/R

# Sections (seconds)
WALK_IN = (0, 22)
WELCOME = (20, 36)
SONG = (34, 112)
SERMON = (110, LENGTH_S)


def window(a, b, fade=0.05):
    """1 inside [a, b) with short fades, 0 outside."""
    w = np.clip((t - a) / fade, 0, 1) * np.clip((b - t) / fade, 0, 1)
    return w


def hz(midi):
    return 440.0 * 2 ** ((midi - 69) / 12)


def tone(freqs, harmonics=(1.0,), vibrato=0.0):
    """Sum of sines following a per-sample frequency curve."""
    f = freqs * (1 + vibrato * np.sin(2 * np.pi * 5.2 * t))
    phase = 2 * np.pi * np.cumsum(f) / SR
    return sum(a * np.sin((k + 1) * phase) for k, a in enumerate(harmonics))


beat = 60 / BPM
bar = 4 * beat
# G - D - Em - C, one bar each, as MIDI roots and triads
PROGRESSION = [(43, [55, 59, 62]), (38, [50, 54, 57]), (40, [52, 55, 59]), (36, [48, 52, 55])]


def chord_at(seconds):
    idx = (np.floor((seconds - SONG[0]) / bar).astype(int)) % 4
    return idx


song = window(*SONG)
chord_idx = chord_at(t)
roots = np.array([hz(r) for r, _ in PROGRESSION])[chord_idx]
triads = np.array([[hz(n) for n in tri] for _, tri in PROGRESSION])[chord_idx]


def speech(active):
    """Voice-ish: a buzzing pitch shaped into syllables and phrases."""
    pitch = 118 + 18 * np.sin(2 * np.pi * 0.31 * t) + 6 * np.sin(2 * np.pi * 2.3 * t)
    voice = tone(pitch, harmonics=(1, 0.6, 0.4, 0.3, 0.2, 0.12, 0.08))
    voice += 0.15 * rng.standard_normal(N)
    syll = np.clip(np.sin(2 * np.pi * 4.1 * t + 2 * np.sin(2 * np.pi * 0.7 * t)), 0, 1) ** 0.6
    phrase = (np.sin(2 * np.pi * 0.23 * t) > -0.55).astype(float)
    return 0.22 * voice * syll * phrase * active


def melody(offset_semitones, level):
    # A simple G major line, two notes per bar.
    line = [67, 69, 71, 69, 66, 67, 64, 66, 64, 67, 64, 62, 60, 62, 64, 67]
    step = np.floor((t - SONG[0]) / (bar / 2)).astype(int) % len(line)
    notes = np.array(line)[step] + offset_semitones
    f = 440.0 * 2 ** ((notes - 69) / 12)
    breath = (((t - SONG[0]) % (bar * 2)) < bar * 1.8).astype(float)
    return level * tone(f, harmonics=(1, 0.5, 0.25, 0.12), vibrato=0.006) * breath * song


def hits(times, length_s, make):
    out = np.zeros(N)
    n = int(length_s * SR)
    env_t = np.arange(n) / SR
    for start in times:
        i = int(start * SR)
        if i >= N:
            break
        seg = make(env_t)[: N - i]
        out[i : i + len(seg)] += seg
    return out


beats = np.arange(SONG[0], SONG[1] - 0.5, beat)
kick = hits(beats[::2], 0.35, lambda e: np.sin(2 * np.pi * (50 + 90 * np.exp(-e * 30)) * e) * np.exp(-e * 9))
snare = hits(beats[1::2], 0.25, lambda e: (rng.standard_normal(len(e)) * 0.7 + np.sin(2 * np.pi * 190 * e)) * np.exp(-e * 18))
eighths = np.arange(SONG[0], SONG[1] - 0.5, beat / 2)
hat = hits(eighths, 0.08, lambda e: rng.standard_normal(len(e)) * np.exp(-e * 60))
oh_l = 0.5 * hat + 0.15 * snare + 0.05 * rng.standard_normal(N) * song
oh_r = 0.5 * np.roll(hat, 24) + 0.15 * snare + 0.05 * rng.standard_normal(N) * song

bass = 0.5 * tone(roots, harmonics=(1, 0.45, 0.2)) * song
keys = 0.18 * sum(tone(triads[:, k], harmonics=(1, 0.3)) for k in range(3)) * song
acous = hits(eighths, beat / 2, lambda e: np.exp(-e * 7)) * 0.2 * sum(
    tone(triads[:, k] * 2, harmonics=(1, 0.5, 0.33, 0.25)) for k in range(3)
)
gtr_on = window(70, 96)
lead = 440.0 * 2 ** ((np.array([74, 76, 78, 79])[(np.floor(t / beat).astype(int)) % 4] - 69) / 12)
elec = 0.25 * np.tanh(3 * tone(lead, harmonics=(1, 0.6, 0.4))) * gtr_on

pad_on = window(*WALK_IN) + 0.6 * song
pad = 0.12 * sum(tone(np.full(N, hz(n)), harmonics=(1, 0.2)) for n in (55, 59, 62, 66))
pad_l = pad * pad_on * (1 + 0.1 * np.sin(2 * np.pi * 0.2 * t))
pad_r = pad * pad_on * (1 + 0.1 * np.cos(2 * np.pi * 0.2 * t))

announce = speech(window(18, 22))

tracks = [
    speech(window(*WELCOME) + window(*SERMON)),
    melody(0, 0.3),
    melody(-3, 0.2),
    melody(4, 0.18),
    0.8 * kick,
    0.5 * snare,
    oh_l,
    oh_r,
    bass,
    elec,
    acous,
    keys,
    np.roll(keys, 120),
    pad_l,
    pad_r,
    announce,
]

# ---- the moves ----
events = []


def ev(t_s, source, kind, ch, value=None):
    e = {"type": kind, "id": {"kind": "input", "index": ch}}
    if kind == "fader":
        e["db"] = None if value is None else round(float(value), 1)
    elif kind == "mute":
        e["muted"] = bool(value)
    elif kind == "name":
        e["name"] = value
    events.append({"tMs": int(t_s * 1000), "source": source, "event": e})


# Start of service: snapshot of the desk.
start_muted = {"Pastor", "Worship Ld", "BGV 1", "BGV 2", "Handheld 1"}
for i, (name, _, db) in enumerate(CHANNELS):
    ev(0, "snapshot", "name", i, name)
    ev(0, "snapshot", "fader", i, db)
    ev(0, "snapshot", "mute", i, name in start_muted)

# Walk-in: announcement on the handheld, then fade the walk-in music.
ev(17.6, "console", "mute", 15, False)
ev(22.4, "console", "mute", 15, True)
for k, db in enumerate(np.linspace(-12, -40, 8)):
    ev(18.5 + k * 0.4, "operator", "fader", 13, db)
    ev(18.5 + k * 0.4, "operator", "fader", 14, db)
ev(22.0, "operator", "fader", 13, None)
ev(22.0, "operator", "fader", 14, None)

# Welcome: pastor unmuted on the desk, Assist nudges them up.
ev(19.8, "console", "mute", 0, False)
for k, db in enumerate([-3.5, -3.0, -2.5]):
    ev(24 + k * 3, "assist", "fader", 0, db)

# Song: vocals unmuted, pastor muted, walk-in faders reset for the pad.
ev(34.2, "console", "mute", 1, False)
ev(34.4, "console", "mute", 2, False)
ev(34.5, "console", "mute", 3, False)
ev(36.0, "console", "mute", 0, True)
ev(35.0, "operator", "fader", 13, -16)
ev(35.0, "operator", "fader", 14, -16)
lead_db = -3.0
for k in range(10):
    lead_db += 0.5 if k % 3 != 2 else -0.5
    ev(40 + k * 6.5, "assist", "fader", 1, lead_db)
for k, db in enumerate([-7.5, -7.0, -7.5, -8.0]):
    ev(48 + k * 12, "assist", "fader", 2, db)
    ev(48.2 + k * 12, "assist", "fader", 3, db - 0.5)
# Guitar solo: the engineer pushes it, then pulls it back.
for k, db in enumerate([-10, -8, -6.5, -6]):
    ev(69 + k * 0.5, "console", "fader", 9, db)
ev(96.5, "console", "fader", 9, -9)
ev(97.2, "console", "fader", 9, -12)
ev(88, "operator", "fader", 4, -5.0)
ev(90, "operator", "fader", 8, -6.0)

# Sermon: band muted, pastor up, Assist rides speech.
ev(110.0, "console", "mute", 0, False)
for ch in (1, 2, 3):
    ev(112.0 + 0.2 * ch, "console", "mute", ch, True)
for k, db in enumerate([-3.0, -2.5, -2.0, -2.5, -2.0, -1.5]):
    ev(116 + k * 5.5, "assist", "fader", 0, db)
ev(118.0, "operator", "fader", 13, None)
ev(118.0, "operator", "fader", 14, None)

events.sort(key=lambda e: (e["tMs"], e["source"] != "snapshot"))
for seq, e in enumerate(events, start=1):
    e["seq"] = seq
    # Field order like the app writes it.
    events[seq - 1] = {"seq": seq, "tMs": e["tMs"], "source": e["source"], "event": e["event"]}

# ---- render the mix with the moves applied ----
def gain_curve(ch):
    g = np.zeros(N)
    db, muted = None, False
    cur_t = 0
    level = 0.0
    for e in events:
        ev_ = e["event"]
        if ev_["id"]["index"] != ch or ev_["type"] not in ("fader", "mute"):
            continue
        i = min(N, e["tMs"] * SR // 1000)
        g[cur_t:i] = level
        cur_t = i
        if ev_["type"] == "fader":
            db = ev_["db"]
        else:
            muted = ev_["muted"]
        level = 0.0 if muted or db is None else 10 ** (db / 20)
    g[cur_t:] = level
    # 10 ms smoothing, like a real desk's fader interpolation.
    k = int(0.01 * SR)
    return np.convolve(g, np.ones(k) / k, mode="same")


mix_l = np.zeros(N)
mix_r = np.zeros(N)
for ch, ((_, pan, _), x) in enumerate(zip(CHANNELS, tracks)):
    g = gain_curve(ch) * x
    mix_l += g * np.sqrt((1 - pan) / 2)
    mix_r += g * np.sqrt((1 + pan) / 2)
peak = max(np.abs(mix_l).max(), np.abs(mix_r).max())
mix_l *= 0.7 / peak
mix_r *= 0.7 / peak


def write_wav(path, channels):
    data = np.clip(np.stack(channels, axis=1), -1, 1)
    ints = np.round(data * (2**23 - 1)).astype("<i4")
    raw = ints.view(np.uint8).reshape(-1, 4)[:, :3].tobytes()  # 24-bit little endian
    path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), "wb") as w:
        w.setnchannels(len(channels))
        w.setsampwidth(3)
        w.setframerate(SR)
        w.writeframes(raw)


def main(out):
    # UUID v7 (time-ordered), like the app's ids.
    ms = int(datetime.now(timezone.utc).timestamp() * 1000)
    rand = int.from_bytes(np.random.default_rng().bytes(10), "big")
    value = (ms << 80) | (0x7 << 76) | ((rand >> 62) & 0xFFF) << 64 | (0b10 << 62) | (rand & ((1 << 62) - 1))
    rid = str(uuid.UUID(int=value))
    bundle = Path(out) / "sanctuarymix-sample-service"
    bundle.mkdir(parents=True, exist_ok=True)

    # Last Sunday, 9:00 AM local.
    now = datetime.now().astimezone()
    sunday = (now - timedelta(days=(now.weekday() + 1) % 7 or 7)).replace(
        hour=9, minute=0, second=0, microsecond=0
    )
    started = int(sunday.timestamp() * 1000)

    for i, x in enumerate(tracks):
        peak = np.abs(x).max() or 1
        write_wav(bundle / "tracks" / f"in-{i + 1:02d}.wav", [x * min(1, 0.8 / peak)])
    write_wav(bundle / "mix.wav", [mix_l, mix_r])

    with open(bundle / "events.jsonl", "w") as f:
        for e in events:
            f.write(json.dumps(e, separators=(",", ":")) + "\n")

    manifest = {
        "id": rid,
        "orgId": None,
        "createdBy": "local",
        "title": "Sample service (test)",
        "serviceDate": sunday.strftime("%Y-%m-%d"),
        "startedAt": started,
        "endedAt": started + LENGTH_S * 1000,
        "durationMs": LENGTH_S * 1000,
        "status": "complete",
        "consoleModel": "dlive",
        "sampleRate": SR,
        "audioMode": "stereoMultitrack",
        "mixChannels": MIX_INPUTS,
        "trackCount": len(CHANNELS),
        "bytesOnDisk": 0,
        "notes": "Synthetic test service: walk-in, welcome, one song with a guitar solo, "
        "and the start of the sermon. Every fader and mute move is baked into the mix.",
        "syncState": "localOnly",
        "remoteKey": None,
        "createdAt": started,
        "updatedAt": started + LENGTH_S * 1000,
        "deletedAt": None,
    }
    (bundle / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    moves = sum(1 for e in events if e["source"] != "snapshot")
    print(f"{bundle}  id={rid}  {len(CHANNELS)} tracks, {moves} moves")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else ".")
