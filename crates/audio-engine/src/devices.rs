use cpal::traits::{DeviceTrait, HostTrait};
use serde::Serialize;

use crate::permission::ensure_input_access;
use crate::{AudioError, Result};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDeviceInfo {
    pub name: String,
    pub max_input_channels: u16,
    pub default_sample_rate: u32,
    pub is_default: bool,
    /// Dante Virtual Soundcard or a Dante interface; the Setup screen promotes these.
    pub is_dante: bool,
}

/// Fails with [`AudioError::PermissionDenied`] rather than touching inputs the
/// user hasn't allowed, which on macOS would show the prompt again.
pub fn list_input_devices() -> Result<Vec<AudioDeviceInfo>> {
    ensure_input_access()?;
    let host = cpal::default_host();
    let default_name = host.default_input_device().and_then(|d| d.name().ok());
    let mut out = Vec::new();
    for device in host.input_devices()? {
        let Ok(name) = device.name() else { continue };
        let Ok(default_config) = device.default_input_config() else {
            continue;
        };
        let max_input_channels = device
            .supported_input_configs()
            .map(|configs| configs.map(|c| c.channels()).max().unwrap_or(0))
            .unwrap_or(default_config.channels());
        out.push(AudioDeviceInfo {
            is_default: default_name.as_deref() == Some(name.as_str()),
            is_dante: name.to_lowercase().contains("dante"),
            name,
            max_input_channels,
            default_sample_rate: default_config.sample_rate().0,
        });
    }
    // Dante first, then the system default, then alphabetical.
    out.sort_by(|a, b| {
        b.is_dante
            .cmp(&a.is_dante)
            .then(b.is_default.cmp(&a.is_default))
            .then(a.name.cmp(&b.name))
    });
    Ok(out)
}

pub(crate) fn find_input_device(name: Option<&str>) -> Result<cpal::Device> {
    ensure_input_access()?;
    let host = cpal::default_host();
    match name {
        None => host
            .default_input_device()
            .ok_or(AudioError::NoDefaultDevice),
        Some(wanted) => host
            .input_devices()?
            .find(|d| d.name().map(|n| n == wanted).unwrap_or(false))
            .ok_or_else(|| AudioError::DeviceNotFound(wanted.to_string())),
    }
}
