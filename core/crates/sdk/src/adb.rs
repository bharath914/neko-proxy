//! USB for Android: while ingest is on, forward the ingest port of every
//! attached device to the desktop with `adb reverse`, so the app reaches
//! `localhost:<port>` with no Wi-Fi involved.
//!
//! Only two adb commands are ever run: `adb devices` and `adb reverse`.

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum AdbStatus {
    /// Not watching (ingest is off).
    #[default]
    Off,
    /// No adb binary found; everything else keeps working.
    Unavailable,
    /// Watching. Serials that currently have the reverse in place.
    Ready {
        devices: Vec<String>,
    },
    Error(String),
}

impl AdbStatus {
    pub fn label(&self) -> String {
        match self {
            AdbStatus::Off => "adb off".into(),
            AdbStatus::Unavailable => "adb not found".into(),
            AdbStatus::Ready { devices } if devices.is_empty() => "adb ready".into(),
            AdbStatus::Ready { devices } => format!("adb ready, {} device(s)", devices.len()),
            AdbStatus::Error(e) => format!("adb error: {e}"),
        }
    }
}

/// `PATH`, then `ANDROID_HOME`/`ANDROID_SDK_ROOT` `platform-tools`.
pub fn find_adb(path: Option<&str>, android_home: &[String]) -> Option<PathBuf> {
    let exe = if cfg!(windows) { "adb.exe" } else { "adb" };
    if let Some(p) = path {
        for dir in std::env::split_paths(p) {
            let c = dir.join(exe);
            if c.is_file() {
                return Some(c);
            }
        }
    }
    for home in android_home {
        let c = PathBuf::from(home).join("platform-tools").join(exe);
        if c.is_file() {
            return Some(c);
        }
    }
    None
}

/// Serials in state `device` from `adb devices` output. Offline and
/// unauthorized devices are skipped.
pub fn parse_devices(out: &str) -> Vec<String> {
    out.lines()
        .skip_while(|l| !l.starts_with("List of devices"))
        .skip(1)
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let serial = it.next()?;
            (it.next()? == "device").then(|| serial.to_string())
        })
        .collect()
}

pub struct AdbWatcher {
    status: Arc<Mutex<AdbStatus>>,
    stop: Arc<AtomicBool>,
}

impl AdbWatcher {
    pub fn start(port: u16) -> AdbWatcher {
        Self::start_with(port, None)
    }

    /// `adb` overrides the lookup (for tests).
    pub fn start_with(port: u16, adb: Option<PathBuf>) -> AdbWatcher {
        let status = Arc::new(Mutex::new(AdbStatus::Off));
        let stop = Arc::new(AtomicBool::new(false));
        let (s, st) = (status.clone(), stop.clone());
        let homes: Vec<String> = ["ANDROID_HOME", "ANDROID_SDK_ROOT"]
            .iter()
            .filter_map(|k| std::env::var(k).ok())
            .collect();
        let _ = std::thread::Builder::new()
            .name("sdk-adb".into())
            .spawn(move || {
                let adb = adb.or_else(|| find_adb(std::env::var("PATH").ok().as_deref(), &homes));
                let Some(adb) = adb else {
                    *s.lock().unwrap() = AdbStatus::Unavailable;
                    return;
                };
                let mut done: HashSet<String> = HashSet::new();
                while !st.load(Ordering::Relaxed) {
                    match Command::new(&adb)
                        .arg("devices")
                        .stdin(Stdio::null())
                        .stderr(Stdio::null())
                        .output()
                    {
                        Ok(o) if o.status.success() => {
                            let serials = parse_devices(&String::from_utf8_lossy(&o.stdout));
                            done.retain(|d| serials.contains(d));
                            for serial in &serials {
                                if done.contains(serial) {
                                    continue;
                                }
                                let spec = format!("tcp:{port}");
                                let ok = Command::new(&adb)
                                    .args(["-s", serial, "reverse", &spec, &spec])
                                    .stdin(Stdio::null())
                                    .stdout(Stdio::null())
                                    .stderr(Stdio::null())
                                    .status()
                                    .map(|s| s.success())
                                    .unwrap_or(false);
                                if ok {
                                    done.insert(serial.clone());
                                }
                            }
                            let mut devices: Vec<String> = done.iter().cloned().collect();
                            devices.sort();
                            *s.lock().unwrap() = AdbStatus::Ready { devices };
                        }
                        Ok(_) => *s.lock().unwrap() = AdbStatus::Error("adb devices failed".into()),
                        Err(e) => *s.lock().unwrap() = AdbStatus::Error(e.to_string()),
                    }
                    for _ in 0..30 {
                        if st.load(Ordering::Relaxed) {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
            });
        AdbWatcher { status, stop }
    }

    pub fn status(&self) -> AdbStatus {
        self.status.lock().unwrap().clone()
    }
}

impl Drop for AdbWatcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(all(test, unix))]
mod fake_adb {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A shell script standing in for adb: lists one ready device, one
    /// unauthorized one, and logs every invocation.
    #[test]
    fn reverses_ready_devices_and_runs_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("log");
        let script = dir.path().join("adb");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\necho \"$@\" >> {}\nif [ \"$1\" = devices ]; then printf 'List of devices attached\\nR58M\\tdevice\\nBAD\\tunauthorized\\n'; fi\n",
                log.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let w = AdbWatcher::start_with(8897, Some(script));
        for _ in 0..100 {
            if matches!(w.status(), AdbStatus::Ready { ref devices } if !devices.is_empty()) {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(
            w.status(),
            AdbStatus::Ready {
                devices: vec!["R58M".into()]
            }
        );
        std::thread::sleep(Duration::from_millis(200));
        drop(w);
        let calls: Vec<String> = std::fs::read_to_string(&log)
            .unwrap()
            .lines()
            .map(String::from)
            .collect();
        assert!(calls.contains(&"devices".to_string()));
        assert_eq!(
            calls
                .iter()
                .filter(|c| c.as_str() == "-s R58M reverse tcp:8897 tcp:8897")
                .count(),
            1
        );
        for c in &calls {
            assert!(
                c == "devices" || c.contains(" reverse tcp:"),
                "unexpected adb call: {c}"
            );
        }
    }

    #[test]
    fn a_missing_adb_is_reported_not_fatal() {
        let w = AdbWatcher::start_with(8897, Some(PathBuf::from("/nonexistent/adb")));
        for _ in 0..100 {
            if w.status() != AdbStatus::Off {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(matches!(w.status(), AdbStatus::Error(_)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_adb_devices() {
        let out = "List of devices attached\nemulator-5554\tdevice\nABC123\tunauthorized\nDEF456\toffline\n192.168.1.9:5555\tdevice\n\n";
        assert_eq!(
            parse_devices(out),
            vec!["emulator-5554", "192.168.1.9:5555"]
        );
        assert!(parse_devices("List of devices attached\n\n").is_empty());
        // The daemon banner that adb prints on first use is skipped.
        let out = "* daemon not running; starting now at tcp:5037\n* daemon started successfully\nList of devices attached\nX\tdevice\n";
        assert_eq!(parse_devices(out), vec!["X"]);
    }

    #[test]
    fn finds_adb_on_path_then_android_home() {
        let dir = tempfile::tempdir().unwrap();
        let tools = dir.path().join("platform-tools");
        std::fs::create_dir_all(&tools).unwrap();
        std::fs::write(tools.join("adb"), "").unwrap();
        let home = dir.path().display().to_string();
        assert_eq!(
            find_adb(Some("/nonexistent"), std::slice::from_ref(&home)),
            Some(tools.join("adb"))
        );
        assert_eq!(
            find_adb(Some(&tools.display().to_string()), &[]),
            Some(tools.join("adb"))
        );
        assert_eq!(find_adb(Some("/nonexistent"), &[]), None);
    }
}
