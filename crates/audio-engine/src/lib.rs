//! Audio input for SanctuaryMix.
//!
//! Console audio reaches the computer over Dante. On a Mac, Dante Virtual
//! Soundcard (or a Dante interface) appears as an ordinary CoreAudio device
//! with up to 64 inputs; on Windows it appears through WASAPI/ASIO. `cpal`
//! covers both, so this crate has no platform-specific code.
//!
//! The real-time callback only does lock-free accumulation ([`meter::MeterBank`]);
//! a separate thread publishes [`mix_core::MeterFrame`]s at ~30 Hz. Raw audio
//! for slower analysis (the listening models today; spectrum and feedback
//! detection later) comes off the same callback through [`tap::AudioTap`].

pub mod devices;
pub mod engine;
pub mod meter;
pub mod permission;
pub mod tap;

pub use devices::{list_input_devices, AudioDeviceInfo};
pub use engine::{start_metering, MeterHandle};
pub use permission::{microphone_access, request_microphone_access, MicAccess};
pub use tap::AudioBlock;

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("no audio input device named {0:?}")]
    DeviceNotFound(String),
    #[error("no default audio input device")]
    NoDefaultDevice,
    #[error("unsupported sample format {0}")]
    UnsupportedFormat(String),
    #[error(
        "SanctuaryMix isn't allowed to hear audio inputs. Turn it on in System Settings › Privacy & Security › Microphone."
    )]
    PermissionDenied,
    #[error("audio backend error: {0}")]
    Backend(String),
}

pub type Result<T> = std::result::Result<T, AudioError>;

macro_rules! backend_err {
    ($($ty:ty),*) => {$(
        impl From<$ty> for AudioError {
            fn from(e: $ty) -> Self {
                AudioError::Backend(e.to_string())
            }
        }
    )*};
}
backend_err!(
    cpal::DevicesError,
    cpal::DeviceNameError,
    cpal::DefaultStreamConfigError,
    cpal::SupportedStreamConfigsError,
    cpal::BuildStreamError,
    cpal::PlayStreamError
);
