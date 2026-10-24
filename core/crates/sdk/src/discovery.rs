//! Advertise the ingest service on the LAN as `_nekoinspect._tcp` while it
//! is enabled. TXT records: `v=1`, `fp=<first 16 hex of the fingerprint>`,
//! `name=<desktop name>`.

use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::sync::Mutex;

pub const SERVICE_TYPE: &str = "_nekoinspect._tcp.local.";

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum DiscoveryStatus {
    #[default]
    Off,
    Advertising,
    Error(String),
}

pub struct Advertiser {
    daemon: Option<ServiceDaemon>,
    fullname: Option<String>,
    status: Mutex<DiscoveryStatus>,
}

impl Advertiser {
    pub fn start(port: u16, fp_short: &str, desktop_name: &str) -> Advertiser {
        match Self::try_start(port, fp_short, desktop_name) {
            Ok((daemon, fullname)) => Advertiser {
                daemon: Some(daemon),
                fullname: Some(fullname),
                status: Mutex::new(DiscoveryStatus::Advertising),
            },
            Err(e) => {
                tracing::warn!("mDNS advertisement failed: {e}");
                Advertiser {
                    daemon: None,
                    fullname: None,
                    status: Mutex::new(DiscoveryStatus::Error(e)),
                }
            }
        }
    }

    fn try_start(
        port: u16,
        fp_short: &str,
        desktop_name: &str,
    ) -> Result<(ServiceDaemon, String), String> {
        let daemon = ServiceDaemon::new().map_err(|e| e.to_string())?;
        let instance = sanitize(desktop_name);
        let host = format!("{instance}.local.");
        let props = [("v", "1"), ("fp", fp_short), ("name", desktop_name)];
        let info = ServiceInfo::new(SERVICE_TYPE, &instance, &host, "", port, &props[..])
            .map_err(|e| e.to_string())?
            .enable_addr_auto();
        let fullname = info.get_fullname().to_string();
        daemon.register(info).map_err(|e| e.to_string())?;
        Ok((daemon, fullname))
    }

    pub fn status(&self) -> DiscoveryStatus {
        self.status.lock().unwrap().clone()
    }
}

impl Drop for Advertiser {
    fn drop(&mut self) {
        if let (Some(d), Some(name)) = (&self.daemon, &self.fullname) {
            if let Ok(rx) = d.unregister(name) {
                let _ = rx.recv_timeout(std::time::Duration::from_millis(500));
            }
            let _ = d.shutdown();
        }
    }
}

fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let s = s.trim_matches('-');
    if s.is_empty() {
        "neko-desktop".into()
    } else {
        s.chars().take(40).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_names_are_safe() {
        assert_eq!(sanitize("Asha's Laptop"), "Asha-s-Laptop");
        assert_eq!(sanitize("  "), "neko-desktop");
        assert_eq!(sanitize(&"x".repeat(100)).len(), 40);
    }
}
