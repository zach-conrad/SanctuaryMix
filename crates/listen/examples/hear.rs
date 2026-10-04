//! Prints what the listening models hear in a WAV file, once a second.
//!
//!     cargo run --release -p listen --example hear -- recording.wav [channel]
//!
//! Handy for checking a church's own multitrack recordings: point it at one
//! channel of a Dante virtual soundcheck capture.

use std::time::Duration;

use listen::{ListenTarget, Listener, Models};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: hear <file.wav> [channel]");
    let channel: u16 = args
        .next()
        .map_or(0, |c| c.parse().expect("channel number"));
    let mut reader = hound::WavReader::open(&path).expect("open wav");
    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap()).collect(),
        hound::SampleFormat::Int => {
            let scale = (1u64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.unwrap() as f32 / scale)
                .collect()
        }
    };
    let (tx, rx) = std::sync::mpsc::channel();
    let listener = Listener::start(Models::load().expect("load models"), move |frame| {
        let _ = tx.send(frame);
    });
    listener.set_targets(vec![ListenTarget {
        channel,
        voice: true,
    }]);
    // Feed in real time, 10 ms blocks, so results line up with the clock.
    let block = spec.sample_rate as usize / 100 * spec.channels as usize;
    let start = std::time::Instant::now();
    let mut next_print = Duration::ZERO;
    for (i, chunk) in samples.chunks(block).enumerate() {
        while !listener.push_audio(spec.channels, spec.sample_rate, chunk) {
            std::thread::sleep(Duration::from_millis(1));
        }
        let at = Duration::from_millis(10 * (i as u64 + 1));
        if let Some(wait) = at.checked_sub(start.elapsed()) {
            std::thread::sleep(wait);
        }
        while let Ok(frame) = rx.try_recv() {
            if at >= next_print {
                next_print += Duration::from_secs(1);
                if let Some(h) = frame.channels.first() {
                    println!(
                        "{:>5.1}s voice {:<5} ({:.2})  sound {:?} ({:.2})",
                        at.as_secs_f32(),
                        h.voice,
                        h.voice_prob,
                        h.sound,
                        h.confidence
                    );
                }
            }
        }
    }
    for h in listener.heard() {
        println!(
            "mostly {:?} ({:.0}% of {:.0} s)",
            h.sound,
            h.share * 100.0,
            h.seconds
        );
    }
}
