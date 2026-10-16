//! Per device and app capture settings, pushed to the SDK as `Config`.

use crate::wire;
use serde::{Deserialize, Serialize};

pub const DEFAULT_BODY_CAP: u32 = 1 << 20;
pub const DEFAULT_EAGER_BODY: u32 = 64 << 10;
const MAX_BODY_CAP: u32 = 8 << 20;
const MAX_LIST: usize = 100;
const MAX_ENTRY: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Redaction {
    #[default]
    Default,
    Strict,
    None,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeviceConfig {
    pub capture_enabled: bool,
    pub host_allow: Vec<String>,
    pub host_deny: Vec<String>,
    pub body_cap_bytes: u32,
    pub eager_body_bytes: u32,
    pub redaction: Redaction,
    pub extra_redact_keys: Vec<String>,
}

impl Default for DeviceConfig {
    fn default() -> Self {
        DeviceConfig {
            capture_enabled: true,
            host_allow: Vec::new(),
            host_deny: Vec::new(),
            body_cap_bytes: DEFAULT_BODY_CAP,
            eager_body_bytes: DEFAULT_EAGER_BODY,
            redaction: Redaction::Default,
            extra_redact_keys: Vec::new(),
        }
    }
}

impl DeviceConfig {
    /// Bring values into range. Settings come from the desktop app, but the
    /// SDK is the one that has to live with them.
    pub fn sanitized(mut self) -> DeviceConfig {
        let clean = |v: &mut Vec<String>| {
            v.iter_mut().for_each(|s| {
                *s = s.trim().chars().take(MAX_ENTRY).collect();
            });
            v.retain(|s| !s.is_empty());
            v.truncate(MAX_LIST);
        };
        clean(&mut self.host_allow);
        clean(&mut self.host_deny);
        clean(&mut self.extra_redact_keys);
        self.body_cap_bytes = self.body_cap_bytes.clamp(1024, MAX_BODY_CAP);
        self.eager_body_bytes = self.eager_body_bytes.min(self.body_cap_bytes);
        self
    }

    pub fn to_wire(&self) -> wire::Config {
        wire::Config {
            capture_enabled: self.capture_enabled,
            host_allow: self.host_allow.clone(),
            host_deny: self.host_deny.clone(),
            body_cap_bytes: self.body_cap_bytes,
            eager_body_bytes: self.eager_body_bytes,
            redaction: match self.redaction {
                Redaction::Default => wire::config::Redaction::Default,
                Redaction::Strict => wire::config::Redaction::Strict,
                Redaction::None => wire::config::Redaction::None,
            } as i32,
            extra_redact_keys: self.extra_redact_keys.clone(),
        }
    }

    pub fn from_wire(c: &wire::Config) -> DeviceConfig {
        DeviceConfig {
            capture_enabled: c.capture_enabled,
            host_allow: c.host_allow.clone(),
            host_deny: c.host_deny.clone(),
            body_cap_bytes: c.body_cap_bytes,
            eager_body_bytes: c.eager_body_bytes,
            redaction: match wire::config::Redaction::try_from(c.redaction).unwrap_or_default() {
                wire::config::Redaction::Default => Redaction::Default,
                wire::config::Redaction::Strict => Redaction::Strict,
                wire::config::Redaction::None => Redaction::None,
            },
            extra_redact_keys: c.extra_redact_keys.clone(),
        }
        .sanitized()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_protocol_doc() {
        let c = DeviceConfig::default();
        assert!(c.capture_enabled);
        assert_eq!(c.body_cap_bytes, 1 << 20);
        assert_eq!(c.eager_body_bytes, 64 << 10);
        assert_eq!(c.redaction, Redaction::Default);
        assert_eq!(DeviceConfig::from_wire(&c.to_wire()), c);
    }

    #[test]
    fn sanitizing_clamps_and_trims() {
        let c = DeviceConfig {
            body_cap_bytes: u32::MAX,
            eager_body_bytes: u32::MAX,
            host_allow: vec!["  a.test ".into(), "".into(), "x".repeat(1000)],
            ..Default::default()
        }
        .sanitized();
        assert_eq!(c.body_cap_bytes, 8 << 20);
        assert_eq!(c.eager_body_bytes, 8 << 20);
        assert_eq!(c.host_allow.len(), 2);
        assert_eq!(c.host_allow[0], "a.test");
        assert_eq!(c.host_allow[1].len(), 256);
    }

    #[test]
    fn json_with_missing_fields_uses_defaults() {
        let c: DeviceConfig = serde_json::from_str(r#"{"redaction":"strict"}"#).unwrap();
        assert_eq!(c.redaction, Redaction::Strict);
        assert!(c.capture_enabled);
    }
}
