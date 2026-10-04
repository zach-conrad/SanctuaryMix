//! Hands raw audio from the real-time callback to slower analysis (the
//! listening models) without ever blocking or allocating on the audio thread.

use std::sync::Mutex;

/// How much audio the tap holds between reads.
const CAPACITY_SECS: usize = 1;

/// A block of interleaved samples, as captured.
pub struct AudioBlock<'a> {
    pub channels: u16,
    pub sample_rate: u32,
    pub samples: &'a [f32],
}

pub struct AudioTap {
    buf: Mutex<Vec<f32>>,
    capacity: usize,
}

impl AudioTap {
    pub fn new(channels: usize, sample_rate: u32) -> Self {
        let capacity = channels * sample_rate as usize * CAPACITY_SECS;
        Self {
            buf: Mutex::new(Vec::with_capacity(capacity)),
            capacity,
        }
    }

    /// Called from the audio callback. If the reader holds the lock or the
    /// buffer is full, the block is skipped: analysis can miss audio, the
    /// audio thread can never wait.
    pub fn push_interleaved(&self, data: &[f32]) {
        if let Ok(mut buf) = self.buf.try_lock() {
            if buf.len() + data.len() <= self.capacity {
                buf.extend_from_slice(data);
            }
        }
    }

    /// Swaps out everything captured so far. `spare` comes back empty and is
    /// reused next time, so steady state allocates nothing.
    pub fn take(&self, spare: &mut Vec<f32>) {
        spare.clear();
        if spare.capacity() < self.capacity {
            spare.reserve(self.capacity - spare.capacity());
        }
        let mut buf = self.buf.lock().unwrap_or_else(|p| p.into_inner());
        std::mem::swap(&mut *buf, spare);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hands_over_what_was_pushed_and_drops_overflow() {
        let tap = AudioTap::new(2, 4);
        tap.push_interleaved(&[1.0, 2.0, 3.0, 4.0]);
        tap.push_interleaved(&[5.0, 6.0, 7.0, 8.0, 9.0, 10.0]); // would exceed 8
        tap.push_interleaved(&[5.0, 6.0]);
        let mut out = Vec::new();
        tap.take(&mut out);
        assert_eq!(out, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        let mut again = Vec::new();
        tap.take(&mut again);
        assert!(again.is_empty());
    }
}
