//! Settings the person controls from the app, remembered across restarts.
//!
//! The defaults are the zero-setup path: the listener is on, USB phones, emulators and simulators (connections that
//! come from this computer) are accepted without a prompt, and phones on Wi-Fi are found and still need an Allow.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const FILE: &str = "sdk-settings.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SdkSettings {
    /// Accept SDK connections at all. On at every start unless turned off.
    pub enabled: bool,
    /// Also listen on the network and advertise over mDNS, so phones on Wi-Fi can find this computer. Off means
    /// loopback only: USB, emulators and simulators still work.
    pub lan: bool,
    /// Pair connections that come from this computer (adb reverse, emulator, simulator) without a prompt. Phones
    /// on Wi-Fi always show the prompt with the six-digit code.
    pub auto_accept_local: bool,
}

impl Default for SdkSettings {
    fn default() -> Self {
        SdkSettings {
            enabled: true,
            lan: true,
            auto_accept_local: true,
        }
    }
}

impl SdkSettings {
    pub fn path(dir: &Path) -> PathBuf {
        dir.join(FILE)
    }

    /// The saved settings, or the defaults when there are none or the file is unreadable.
    pub fn load(dir: &Path) -> SdkSettings {
        std::fs::read(Self::path(dir))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join(format!("{FILE}.tmp"));
        std::fs::write(&tmp, serde_json::to_vec_pretty(self).unwrap_or_default())?;
        std::fs::rename(tmp, Self::path(dir))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_zero_setup_path() {
        let s = SdkSettings::default();
        assert!(s.enabled && s.lan && s.auto_accept_local);
    }

    #[test]
    fn round_trip_and_forgiving_loading() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(SdkSettings::load(dir.path()), SdkSettings::default());
        let s = SdkSettings { enabled: false, lan: false, auto_accept_local: true };
        s.save(dir.path()).unwrap();
        assert_eq!(SdkSettings::load(dir.path()), s);
        std::fs::write(SdkSettings::path(dir.path()), b"not json").unwrap();
        assert_eq!(SdkSettings::load(dir.path()), SdkSettings::default());
        // A file from an older version with fewer fields keeps the defaults for the rest.
        std::fs::write(SdkSettings::path(dir.path()), br#"{"lan": false}"#).unwrap();
        let p = SdkSettings::load(dir.path());
        assert!(!p.lan && p.enabled && p.auto_accept_local);
    }
}
