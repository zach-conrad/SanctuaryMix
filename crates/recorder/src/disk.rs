//! Disk size estimates and the free-space guards from `docs/RECORDINGS.md`.
//!
//! All audio is 24-bit PCM WAV, so one sample is 3 bytes.

use std::io;
use std::path::Path;

use crate::AudioMode;

/// Bytes per 24-bit sample.
pub const BYTES_PER_SAMPLE: u64 = 3;
/// Multitrack only starts with room for this long.
pub const MULTITRACK_MIN_SECS: u64 = 3 * 3600;
/// The stereo mix only starts with room for this long.
pub const STEREO_MIN_SECS: u64 = 3600;
/// Below this, multitrack stops and the mix and control log keep going.
pub const STOP_MULTITRACK_BELOW: u64 = 5_000_000_000;
/// Below this, everything stops and files are finalized.
pub const STOP_ALL_BELOW: u64 = 1_000_000_000;

/// Bytes per second of the stereo mix.
pub const fn stereo_bytes_per_sec(sample_rate: u32) -> u64 {
    sample_rate as u64 * 2 * BYTES_PER_SAMPLE
}

/// Bytes per second of multitrack (one mono file per channel).
pub const fn multitrack_bytes_per_sec(channels: u16, sample_rate: u32) -> u64 {
    channels as u64 * sample_rate as u64 * BYTES_PER_SAMPLE
}

/// What a recording is about to capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordPlan {
    pub audio_mode: AudioMode,
    pub sample_rate: u32,
    /// Multitrack channels (ignored unless `audio_mode` is `StereoMultitrack`).
    pub track_count: u16,
}

impl RecordPlan {
    /// Total audio bytes per second for this plan.
    pub const fn bytes_per_sec(&self) -> u64 {
        match self.audio_mode {
            AudioMode::None => 0,
            AudioMode::Stereo => stereo_bytes_per_sec(self.sample_rate),
            AudioMode::StereoMultitrack => {
                stereo_bytes_per_sec(self.sample_rate)
                    + multitrack_bytes_per_sec(self.track_count, self.sample_rate)
            }
        }
    }

    /// Estimated bytes for `secs` of recording.
    pub const fn estimate_bytes(&self, secs: u64) -> u64 {
        self.bytes_per_sec() * secs
    }
}

/// Free bytes available to this user on the disk holding `path`. If `path`
/// does not exist yet, its nearest existing parent is checked.
pub fn free_bytes(path: &Path) -> io::Result<u64> {
    let mut p = path;
    while !p.exists() {
        p = p
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no existing folder"))?;
    }
    fs4::available_space(p)
}

/// Checks there is room to start `plan`. The error is a plain sentence for
/// the operator.
pub fn check_start(free: u64, plan: &RecordPlan) -> Result<(), String> {
    if plan.audio_mode == AudioMode::None {
        return Ok(());
    }
    let stereo_rate = stereo_bytes_per_sec(plan.sample_rate).max(1);
    let stereo_room = free.saturating_sub(STOP_ALL_BELOW) / stereo_rate;
    if stereo_room < STEREO_MIN_SECS {
        return Err(format!(
            "There's room for {} of the stereo mix. Free up space to record audio.",
            about(stereo_room)
        ));
    }
    if plan.audio_mode == AudioMode::StereoMultitrack {
        let room = free.saturating_sub(STOP_MULTITRACK_BELOW) / plan.bytes_per_sec().max(1);
        if room < MULTITRACK_MIN_SECS {
            return Err(format!(
                "There's room for {} of multitrack. Free up space or record the stereo mix only.",
                about(room)
            ));
        }
    }
    Ok(())
}

/// What a running recording should do at this much free space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskAction {
    Continue,
    /// Stop multitrack; keep the mix and the control log.
    StopMultitrack,
    /// Stop everything and finalize the files.
    StopAll,
}

pub fn check_running(free: u64, multitrack_active: bool) -> DiskAction {
    if free < STOP_ALL_BELOW {
        DiskAction::StopAll
    } else if multitrack_active && free < STOP_MULTITRACK_BELOW {
        DiskAction::StopMultitrack
    } else {
        DiskAction::Continue
    }
}

/// "about 40 minutes", "about 1 hour 20 minutes", "less than a minute".
fn about(secs: u64) -> String {
    let mins = secs / 60;
    let (h, m) = (mins / 60, mins % 60);
    let plural = |n: u64, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
    match (h, m) {
        (0, 0) => "less than a minute".to_string(),
        (0, m) => format!("about {}", plural(m, "minute")),
        (h, 0) => format!("about {}", plural(h, "hour")),
        (h, m) => format!("about {} {}", plural(h, "hour"), plural(m, "minute")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1_000_000_000;

    fn plan(audio_mode: AudioMode, track_count: u16) -> RecordPlan {
        RecordPlan {
            audio_mode,
            sample_rate: 48_000,
            track_count,
        }
    }

    #[test]
    fn estimates_match_the_spec() {
        // About 1.0 GB per hour of stereo, 0.52 GB per channel-hour.
        assert_eq!(stereo_bytes_per_sec(48_000) * 3600, 1_036_800_000);
        assert_eq!(multitrack_bytes_per_sec(1, 48_000) * 3600, 518_400_000);
        let p = plan(AudioMode::StereoMultitrack, 32);
        assert_eq!(p.estimate_bytes(3600), 1_036_800_000 + 32 * 518_400_000);
        assert_eq!(plan(AudioMode::None, 32).bytes_per_sec(), 0);
    }

    #[test]
    fn start_guards() {
        assert!(check_start(0, &plan(AudioMode::None, 0)).is_ok());
        assert!(check_start(3 * GB, &plan(AudioMode::Stereo, 0)).is_ok());
        let err = check_start(GB + 500_000_000, &plan(AudioMode::Stereo, 0)).unwrap_err();
        assert!(err.contains("stereo mix"), "{err}");

        // 32 channels need about 52.9 GB for 3 hours, plus the 5 GB reserve.
        let mt = plan(AudioMode::StereoMultitrack, 32);
        assert!(check_start(60 * GB, &mt).is_ok());
        let err = check_start(5 * GB + 11_760_000_000, &mt).unwrap_err();
        assert_eq!(
            err,
            "There's room for about 40 minutes of multitrack. \
             Free up space or record the stereo mix only."
        );
    }

    #[test]
    fn running_guards() {
        assert_eq!(check_running(10 * GB, true), DiskAction::Continue);
        assert_eq!(check_running(4 * GB, true), DiskAction::StopMultitrack);
        assert_eq!(check_running(4 * GB, false), DiskAction::Continue);
        assert_eq!(check_running(GB / 2, false), DiskAction::StopAll);
    }

    #[test]
    fn wording() {
        assert_eq!(about(30), "less than a minute");
        assert_eq!(about(60), "about 1 minute");
        assert_eq!(about(3600), "about 1 hour");
        assert_eq!(about(2 * 3600 + 600), "about 2 hours 10 minutes");
    }

    #[test]
    fn free_bytes_walks_up_to_an_existing_folder() {
        let dir = tempfile::tempdir().unwrap();
        assert!(free_bytes(&dir.path().join("not/yet/made")).unwrap() > 0);
    }
}
