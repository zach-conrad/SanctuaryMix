//! Microphone (audio input) permission.
//!
//! macOS asks the user before an app may read any audio input, Dante Virtual
//! Soundcard included. Touching CoreAudio inputs while permission is still
//! undecided, or after it was refused, makes macOS show its prompt again, so
//! every input access goes through [`ensure_input_access`] first: it asks at
//! most once at a time, and once the answer is "no" it stops touching the
//! hardware and reports [`AudioError::PermissionDenied`] instead.
//!
//! On Windows and Linux there is no prompt to manage, so access is always
//! reported as granted.

use std::sync::Mutex;

use serde::Serialize;

use crate::{AudioError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MicAccess {
    Granted,
    /// The user hasn't been asked yet.
    Undetermined,
    /// The user said no, or a device policy blocks it.
    Denied,
}

/// Where permission answers come from: the OS in the app, a fake in tests.
pub trait PermissionSource: Sync {
    /// Reads the current answer without prompting.
    fn status(&self) -> MicAccess;
    /// Shows the system prompt and blocks until the user answers.
    fn request(&self) -> bool;
}

/// Serializes permission requests so concurrent callers share one prompt.
pub struct InputGate<P> {
    source: P,
    asking: Mutex<()>,
}

impl<P: PermissionSource> InputGate<P> {
    pub const fn new(source: P) -> Self {
        Self {
            source,
            asking: Mutex::new(()),
        }
    }

    pub fn status(&self) -> MicAccess {
        self.source.status()
    }

    /// Asks the user if they haven't answered yet; never asks twice.
    pub fn request(&self) -> MicAccess {
        let _one_at_a_time = self.asking.lock().unwrap_or_else(|e| e.into_inner());
        // Re-read under the lock: another caller may have just asked.
        match self.source.status() {
            MicAccess::Undetermined => {
                if self.source.request() {
                    MicAccess::Granted
                } else {
                    MicAccess::Denied
                }
            }
            answered => answered,
        }
    }

    /// Ok only when audio inputs may be touched.
    pub fn ensure(&self) -> Result<()> {
        let access = match self.source.status() {
            MicAccess::Undetermined => self.request(),
            known => known,
        };
        match access {
            MicAccess::Granted => Ok(()),
            _ => Err(AudioError::PermissionDenied),
        }
    }
}

static GATE: InputGate<SystemPermission> = InputGate::new(SystemPermission);

/// Current microphone permission, without prompting.
pub fn microphone_access() -> MicAccess {
    GATE.status()
}

/// Shows the macOS prompt if the user hasn't answered yet. Blocks until they do.
pub fn request_microphone_access() -> MicAccess {
    GATE.request()
}

pub(crate) fn ensure_input_access() -> Result<()> {
    GATE.ensure()
}

pub struct SystemPermission;

#[cfg(target_os = "macos")]
mod system {
    use std::sync::mpsc;

    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};

    use super::{MicAccess, PermissionSource, SystemPermission};

    impl PermissionSource for SystemPermission {
        fn status(&self) -> MicAccess {
            let Some(audio) = (unsafe { AVMediaTypeAudio }) else {
                return MicAccess::Denied;
            };
            match unsafe { AVCaptureDevice::authorizationStatusForMediaType(audio) } {
                AVAuthorizationStatus::Authorized => MicAccess::Granted,
                AVAuthorizationStatus::NotDetermined => MicAccess::Undetermined,
                _ => MicAccess::Denied,
            }
        }

        fn request(&self) -> bool {
            let Some(audio) = (unsafe { AVMediaTypeAudio }) else {
                return false;
            };
            let (tx, rx) = mpsc::channel();
            let done = RcBlock::new(move |granted: Bool| {
                let _ = tx.send(granted.as_bool());
            });
            unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(audio, &done) };
            rx.recv().unwrap_or(false)
        }
    }
}

#[cfg(not(target_os = "macos"))]
impl PermissionSource for SystemPermission {
    fn status(&self) -> MicAccess {
        MicAccess::Granted
    }

    fn request(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use super::*;

    /// Stands in for macOS: remembers the answer and counts prompts.
    struct FakeOs {
        state: Mutex<MicAccess>,
        answer: bool,
        prompts: AtomicUsize,
    }

    impl FakeOs {
        fn new(state: MicAccess, answer: bool) -> Self {
            Self {
                state: Mutex::new(state),
                answer,
                prompts: AtomicUsize::new(0),
            }
        }
        fn prompts(&self) -> usize {
            self.prompts.load(Ordering::SeqCst)
        }
    }

    impl PermissionSource for Arc<FakeOs> {
        fn status(&self) -> MicAccess {
            *self.state.lock().unwrap()
        }
        fn request(&self) -> bool {
            self.prompts.fetch_add(1, Ordering::SeqCst);
            // The user takes a moment to click.
            thread::sleep(Duration::from_millis(20));
            *self.state.lock().unwrap() = if self.answer {
                MicAccess::Granted
            } else {
                MicAccess::Denied
            };
            self.answer
        }
    }

    #[test]
    fn many_callers_share_one_prompt() {
        let os = Arc::new(FakeOs::new(MicAccess::Undetermined, true));
        let gate = Arc::new(InputGate::new(os.clone()));
        let callers: Vec<_> = (0..8)
            .map(|_| {
                let gate = gate.clone();
                thread::spawn(move || gate.ensure())
            })
            .collect();
        for c in callers {
            assert!(c.join().unwrap().is_ok());
        }
        assert_eq!(os.prompts(), 1);
    }

    #[test]
    fn no_prompt_after_allow() {
        let os = Arc::new(FakeOs::new(MicAccess::Undetermined, true));
        let gate = InputGate::new(os.clone());
        assert_eq!(gate.request(), MicAccess::Granted);
        for _ in 0..10 {
            assert!(gate.ensure().is_ok());
            assert_eq!(gate.request(), MicAccess::Granted);
        }
        assert_eq!(os.prompts(), 1);
    }

    #[test]
    fn refusal_stops_asking_and_reports_denied() {
        let os = Arc::new(FakeOs::new(MicAccess::Undetermined, false));
        let gate = InputGate::new(os.clone());
        for _ in 0..10 {
            assert!(matches!(gate.ensure(), Err(AudioError::PermissionDenied)));
        }
        assert_eq!(gate.request(), MicAccess::Denied);
        assert_eq!(os.prompts(), 1);
    }

    #[test]
    fn already_granted_never_prompts() {
        let os = Arc::new(FakeOs::new(MicAccess::Granted, true));
        let gate = InputGate::new(os.clone());
        assert!(gate.ensure().is_ok());
        assert_eq!(os.prompts(), 0);
    }
}
