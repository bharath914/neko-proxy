//! Can an app with the SDK reach this computer right now? One list of checks, each with what is wrong and how to
//! fix it, and one verdict for the status icon. Pure functions of the state, so every case is testable.

use crate::adb::AdbStatus;
use crate::discovery::DiscoveryStatus;
use crate::settings::SdkSettings;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Ok,
    /// Worth knowing, does not stop anything.
    Info,
    /// Some devices will not connect.
    Warn,
    /// Nothing can connect.
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Health {
    /// Turned off by the person.
    Off,
    Ok,
    /// Something needs a person: a pairing request, a blocked phone, a closed firewall.
    Attention,
    /// Nothing can connect.
    Problem,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    pub id: &'static str,
    pub level: Level,
    pub title: String,
    pub detail: String,
    pub fix: String,
}

#[derive(Clone, Debug)]
pub struct Report {
    pub health: Health,
    pub checks: Vec<Check>,
}

/// What the host firewall does with the ingest port. Only firewalld is asked; anything else is `Unknown`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Firewall {
    Unknown,
    Inactive,
    Open,
    Blocked,
}

pub struct Input<'a> {
    pub enabled: bool,
    pub port: u16,
    pub settings: SdkSettings,
    pub adb: &'a AdbStatus,
    pub discovery: &'a DiscoveryStatus,
    /// Why the listener could not start and what to do about it, if it could not start.
    pub bind_error: Option<(&'a str, &'a str)>,
    /// Why the settings could not be saved, if they could not.
    pub settings_error: Option<&'a str>,
    pub lan_addresses: usize,
    pub firewall: Firewall,
    pub pending: usize,
}

fn check(id: &'static str, level: Level, title: &str, detail: &str, fix: &str) -> Check {
    Check {
        id,
        level,
        title: title.into(),
        detail: detail.into(),
        fix: fix.into(),
    }
}

/// Turn an OS error from binding into words a person can act on.
pub fn explain_bind_error(port: u16, e: &std::io::Error) -> (String, String) {
    use std::io::ErrorKind::*;
    match e.kind() {
        AddrInUse => (
            format!("Port {port} is already in use by another program."),
            "Close that program, or start proxyd with another --sdk-port and use the same port in the app.".into(),
        ),
        PermissionDenied => (
            format!("The system does not allow listening on port {port}."),
            "Use a port above 1024 (--sdk-port).".into(),
        ),
        _ => (
            format!("Could not listen on port {port}: {e}"),
            "Turn Accept SDK connections off and on again.".into(),
        ),
    }
}

pub fn evaluate(i: &Input) -> Report {
    let mut checks = Vec::new();
    let port = i.port;

    // Is anything listening?
    if let Some((why, fix)) = i.bind_error {
        checks.push(check("listener", Level::Error, "Cannot accept SDK connections", why, fix));
    } else if !i.enabled {
        checks.push(check(
            "listener",
            Level::Warn,
            "SDK connections are off",
            "Apps with the SDK cannot connect.",
            "Turn on Accept SDK connections in Devices.",
        ));
    } else {
        checks.push(check(
            "listener",
            Level::Ok,
            "Accepting SDK connections",
            &format!("Listening on port {port}."),
            "",
        ));
    }

    if let Some(e) = i.settings_error {
        checks.push(check(
            "settings",
            Level::Warn,
            "Settings cannot be saved",
            &format!("They work now but will reset at the next start: {e}"),
            "Check that the proxy's data folder is writable.",
        ));
    }

    if i.enabled {
        // Same-computer connections: emulator, simulator, adb reverse.
        if i.settings.auto_accept_local {
            checks.push(check(
                "auto-accept",
                Level::Ok,
                "Emulators and USB phones connect on their own",
                "Connections from this computer are accepted without asking.",
                "",
            ));
        } else {
            checks.push(check(
                "auto-accept",
                Level::Info,
                "Every device asks first",
                "Auto-accept for this computer is off, so each new app needs an Allow.",
                "Turn on Auto-accept in Devices to skip the prompt.",
            ));
        }

        match i.adb {
            AdbStatus::Off => {}
            AdbStatus::Unavailable => checks.push(check(
                "adb",
                Level::Info,
                "adb not found",
                "USB phones and Android emulators connect through adb. Wi-Fi phones and iOS simulators do not need it.",
                "Install Android platform-tools or set ANDROID_HOME.",
            )),
            AdbStatus::Error(e) => checks.push(check(
                "adb",
                Level::Warn,
                "adb is not answering",
                e,
                "Run `adb kill-server`, then `adb devices`.",
            )),
            AdbStatus::Ready { devices, blocked } => {
                if blocked.is_empty() {
                    let detail = if devices.is_empty() {
                        "No Android device is plugged in.".to_string()
                    } else {
                        format!("{} device(s) ready.", devices.len())
                    };
                    checks.push(check("adb", Level::Ok, "adb is ready", &detail, ""));
                }
                for (serial, state) in blocked {
                    let (title, fix) = match state.as_str() {
                        "unauthorized" => (
                            format!("{serial} is waiting for USB debugging approval"),
                            "Unlock the phone and tap Allow on the \"Allow USB debugging?\" prompt.",
                        ),
                        "offline" => (
                            format!("{serial} is offline"),
                            "Unplug and replug the cable, or run `adb kill-server`.",
                        ),
                        "no permissions" => (
                            "A USB device is visible but this user may not use it".to_string(),
                            "Install the Android udev rules (android-udev-rules) and make sure you are in the plugdev group, then replug.",
                        ),
                        _ => (
                            format!("Could not connect {serial} to this computer ({state})"),
                            "Replug the cable. If it keeps failing, run `adb kill-server`.",
                        ),
                    };
                    checks.push(check("adb", Level::Warn, &title, "This device cannot send traffic yet.", fix));
                }
            }
        }

        // Phones on Wi-Fi.
        if !i.settings.lan {
            checks.push(check(
                "wifi",
                Level::Info,
                "Wi-Fi phones are off",
                "Listening on this computer only.",
                "Turn on Wi-Fi phones in Devices to connect phones on the network.",
            ));
        } else {
            if i.lan_addresses == 0 {
                checks.push(check(
                    "wifi",
                    Level::Warn,
                    "No network connection",
                    "This computer has no network address, so phones on Wi-Fi cannot reach it.",
                    "Connect to the same Wi-Fi as the phone.",
                ));
            }
            match i.firewall {
                Firewall::Blocked => checks.push(check(
                    "firewall",
                    Level::Warn,
                    &format!("The firewall blocks port {port}"),
                    "Phones on Wi-Fi are stopped before they reach the proxy. USB and emulators are not affected.",
                    &format!(
                        "sudo firewall-cmd --add-port={port}/tcp --permanent && sudo firewall-cmd --reload"
                    ),
                )),
                Firewall::Open => checks.push(check(
                    "firewall",
                    Level::Ok,
                    "The firewall lets phones in",
                    &format!("Port {port} is open."),
                    "",
                )),
                Firewall::Inactive | Firewall::Unknown => {}
            }
            match i.discovery {
                DiscoveryStatus::Advertising => checks.push(check(
                    "discovery",
                    Level::Ok,
                    "Phones on Wi-Fi can find this computer",
                    "Advertising over mDNS.",
                    "",
                )),
                DiscoveryStatus::Error(e) => checks.push(check(
                    "discovery",
                    Level::Warn,
                    "Phones cannot discover this computer",
                    e,
                    "Enter this computer's address in the app by hand, or check that the network allows multicast.",
                )),
                DiscoveryStatus::Off => {}
            }
        }

        if i.pending > 0 {
            checks.push(check(
                "pending",
                Level::Warn,
                &format!("{} pairing request(s) waiting", i.pending),
                "An app is waiting for an answer.",
                "Open Devices and press Allow.",
            ));
        }
    }

    let worst = checks.iter().map(|c| c.level).fold(Level::Ok, |a, b| match (a, b) {
        (Level::Error, _) | (_, Level::Error) => Level::Error,
        (Level::Warn, _) | (_, Level::Warn) => Level::Warn,
        (Level::Info, _) | (_, Level::Info) => Level::Info,
        _ => Level::Ok,
    });
    let health = match worst {
        Level::Error => Health::Problem,
        _ if i.bind_error.is_none() && !i.enabled => Health::Off,
        Level::Warn => Health::Attention,
        _ => Health::Ok,
    };
    Report { health, checks }
}

/// Ask firewalld whether the port is open. Never blocks longer than a second or two, never needs root.
pub fn probe_firewall(port: u16) -> Firewall {
    if !cfg!(target_os = "linux") {
        return Firewall::Unknown;
    }
    match run("firewall-cmd", &["--state"]) {
        Some((true, out)) if out.trim() == "running" => {}
        Some(_) => return Firewall::Inactive,
        None => return Firewall::Unknown,
    }
    let spec = format!("--query-port={port}/tcp");
    match run("firewall-cmd", &[&spec]) {
        Some((true, out)) if out.trim() == "yes" => Firewall::Open,
        Some((_, out)) if out.trim() == "no" => Firewall::Blocked,
        // A polkit refusal or an unexpected answer is not proof of anything.
        _ => Firewall::Unknown,
    }
}

/// Run a command with a hard deadline. `None` if it is missing, cannot run or hangs.
fn run(cmd: &str, args: &[&str]) -> Option<(bool, String)> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + Duration::from_millis(1500);
    loop {
        match child.try_wait().ok()? {
            Some(status) => {
                let mut out = String::new();
                child.stdout.take()?.read_to_string(&mut out).ok()?;
                return Some((status.success(), out));
            }
            None if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base<'a>(adb: &'a AdbStatus, disc: &'a DiscoveryStatus) -> Input<'a> {
        Input {
            enabled: true,
            port: 8897,
            settings: SdkSettings::default(),
            adb,
            discovery: disc,
            bind_error: None,
            settings_error: None,
            lan_addresses: 1,
            firewall: Firewall::Unknown,
            pending: 0,
        }
    }

    fn ready(blocked: Vec<(&str, &str)>) -> AdbStatus {
        AdbStatus::Ready {
            devices: vec![],
            blocked: blocked.into_iter().map(|(a, b)| (a.into(), b.into())).collect(),
        }
    }

    #[test]
    fn a_working_setup_is_ok() {
        let (adb, d) = (ready(vec![]), DiscoveryStatus::Advertising);
        let r = evaluate(&base(&adb, &d));
        assert_eq!(r.health, Health::Ok);
        assert!(r.checks.iter().all(|c| c.level == Level::Ok));
    }

    #[test]
    fn off_is_off_not_a_problem() {
        let (adb, d) = (AdbStatus::Off, DiscoveryStatus::Off);
        let mut i = base(&adb, &d);
        i.enabled = false;
        assert_eq!(evaluate(&i).health, Health::Off);
    }

    #[test]
    fn a_failed_bind_is_a_problem_even_though_it_is_off() {
        let (adb, d) = (AdbStatus::Off, DiscoveryStatus::Off);
        let mut i = base(&adb, &d);
        i.enabled = false;
        i.bind_error = Some(("Port 8897 is already in use by another program.", "Close it."));
        let r = evaluate(&i);
        assert_eq!(r.health, Health::Problem);
        assert_eq!(r.checks[0].id, "listener");
    }

    #[test]
    fn unauthorized_phone_needs_attention_with_the_fix() {
        let (adb, d) = (ready(vec![("R58M", "unauthorized")]), DiscoveryStatus::Advertising);
        let r = evaluate(&base(&adb, &d));
        assert_eq!(r.health, Health::Attention);
        let c = r.checks.iter().find(|c| c.id == "adb").unwrap();
        assert!(c.title.contains("R58M") && c.fix.contains("Allow"));
    }

    #[test]
    fn a_closed_firewall_only_matters_for_wifi() {
        let (adb, d) = (ready(vec![]), DiscoveryStatus::Advertising);
        let mut i = base(&adb, &d);
        i.firewall = Firewall::Blocked;
        let r = evaluate(&i);
        assert_eq!(r.health, Health::Attention);
        assert!(r.checks.iter().any(|c| c.id == "firewall" && c.fix.contains("8897")));
        i.settings.lan = false;
        assert_eq!(evaluate(&i).health, Health::Ok);
    }

    #[test]
    fn missing_adb_is_only_information() {
        let (adb, d) = (AdbStatus::Unavailable, DiscoveryStatus::Advertising);
        assert_eq!(evaluate(&base(&adb, &d)).health, Health::Ok);
    }

    #[test]
    fn a_waiting_prompt_asks_for_attention() {
        let (adb, d) = (ready(vec![]), DiscoveryStatus::Advertising);
        let mut i = base(&adb, &d);
        i.pending = 2;
        assert_eq!(evaluate(&i).health, Health::Attention);
    }

    #[test]
    fn bind_errors_are_explained() {
        let e = std::io::Error::from(std::io::ErrorKind::AddrInUse);
        let (msg, fix) = explain_bind_error(8897, &e);
        assert!(msg.contains("8897") && fix.contains("--sdk-port"));
    }
}
