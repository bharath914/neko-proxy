//! Linux specifics: system appearance, certificate trust helpers, browser
//! launchers, environment snippet. Nothing here changes system settings
//! silently: commands are shown first and run only after confirmation.

use crate::theme::Scheme;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

// ------------------------------------------------------------ appearance

/// What the desktop environment says, refreshed in the background.
#[derive(Clone)]
pub struct SystemAppearance {
    dark: Arc<AtomicBool>,
    reduce_motion: Arc<AtomicBool>,
    high_contrast: Arc<AtomicBool>,
}

impl SystemAppearance {
    pub fn fixed(dark: bool) -> Self {
        SystemAppearance {
            dark: Arc::new(AtomicBool::new(dark)),
            reduce_motion: Arc::new(AtomicBool::new(false)),
            high_contrast: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Read GNOME's settings now and keep watching them.
    pub fn watch(ctx: egui::Context) -> Self {
        let s = SystemAppearance::fixed(true);
        s.refresh();
        let s2 = s.clone();
        std::thread::Builder::new()
            .name("appearance".into())
            .spawn(move || loop {
                std::thread::sleep(Duration::from_secs(2));
                let before = s2.snapshot();
                s2.refresh();
                if before != s2.snapshot() {
                    ctx.request_repaint();
                }
            })
            .ok();
        s
    }

    fn snapshot(&self) -> (bool, bool, bool) {
        (
            self.dark.load(Ordering::Relaxed),
            self.reduce_motion.load(Ordering::Relaxed),
            self.high_contrast.load(Ordering::Relaxed),
        )
    }

    fn refresh(&self) {
        if let Some(v) = gsettings("org.gnome.desktop.interface", "color-scheme") {
            self.dark.store(v.contains("dark"), Ordering::Relaxed);
        }
        if let Some(v) = gsettings("org.gnome.desktop.interface", "enable-animations") {
            self.reduce_motion
                .store(v.trim() == "false", Ordering::Relaxed);
        }
        if let Some(v) = gsettings("org.gnome.desktop.a11y.interface", "high-contrast") {
            self.high_contrast
                .store(v.trim() == "true", Ordering::Relaxed);
        }
    }

    pub fn scheme(&self) -> Scheme {
        if self.dark.load(Ordering::Relaxed) {
            Scheme::Dark
        } else {
            Scheme::Light
        }
    }

    pub fn reduce_motion(&self) -> bool {
        self.reduce_motion.load(Ordering::Relaxed)
    }

    pub fn high_contrast(&self) -> bool {
        self.high_contrast.load(Ordering::Relaxed)
    }
}

fn gsettings(schema: &str, key: &str) -> Option<String> {
    let out = Command::new("gsettings")
        .args(["get", schema, key])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

// ------------------------------------------------------------ distro

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Debian,
    Fedora,
    Arch,
    Unknown,
}

pub fn detect_family() -> Family {
    let text = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    let mut ids = String::new();
    for line in text.lines() {
        if let Some(v) = line
            .strip_prefix("ID=")
            .or_else(|| line.strip_prefix("ID_LIKE="))
        {
            ids.push(' ');
            ids.push_str(v.trim_matches('"'));
        }
    }
    let has = |n: &str| ids.split_whitespace().any(|w| w == n);
    if has("debian") || has("ubuntu") {
        Family::Debian
    } else if has("fedora") || has("rhel") || has("centos") {
        Family::Fedora
    } else if has("arch") {
        Family::Arch
    } else {
        Family::Unknown
    }
}

impl Family {
    pub fn label(self) -> &'static str {
        match self {
            Family::Debian => "Debian / Ubuntu",
            Family::Fedora => "Fedora / RHEL",
            Family::Arch => "Arch",
            Family::Unknown => "this system",
        }
    }
}

// ------------------------------------------------------------ trust

#[derive(Clone, Debug)]
pub struct TrustPlan {
    pub title: &'static str,
    pub detail: &'static str,
    /// Exactly what will run, as shown to the person.
    pub display: String,
    /// Shell run through pkexec when confirmed. `None` for guidance only.
    pub run: Option<String>,
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// System trust store command for the detected distro.
pub fn system_plan(family: Family, ca_path: &str) -> TrustPlan {
    let ca = sh_quote(ca_path);
    let (display, run) = match family {
        Family::Debian => (
            format!("sudo cp {ca} /usr/local/share/ca-certificates/neko-ca.crt && sudo update-ca-certificates"),
            format!("cp {ca} /usr/local/share/ca-certificates/neko-ca.crt && update-ca-certificates"),
        ),
        Family::Fedora => (
            format!("sudo cp {ca} /etc/pki/ca-trust/source/anchors/neko-ca.pem && sudo update-ca-trust"),
            format!("cp {ca} /etc/pki/ca-trust/source/anchors/neko-ca.pem && update-ca-trust"),
        ),
        Family::Arch => (
            format!("sudo trust anchor --store {ca}"),
            format!("trust anchor --store {ca}"),
        ),
        Family::Unknown => {
            return TrustPlan {
                title: "System certificate store",
                detail: "Your distribution was not recognized. Add the certificate to your system trust store using its own tooling.",
                display: format!("# Certificate: {ca_path}"),
                run: None,
            }
        }
    };
    TrustPlan {
        title: "System certificate store",
        detail: "Trusted by curl, wget, Go, Python and most command-line tools. Needs administrator approval.",
        display,
        run: Some(run),
    }
}

pub fn nss_plan(ca_path: &str) -> TrustPlan {
    let home = std::env::var("HOME").unwrap_or_default();
    let ca = sh_quote(ca_path);
    let db = format!("sql:{home}/.pki/nssdb");
    TrustPlan {
        title: "Chrome, Chromium and Edge",
        detail: "These browsers read your personal NSS database. No administrator rights needed.",
        display: format!("certutil -d {db} -A -t \"C,,\" -n neko-ca -i {ca}"),
        run: None,
    }
}

pub fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(bin))
        .find(|p| p.is_file())
}

/// Is this certificate already in a system bundle? Looks for its PEM body in
/// the well-known extracted bundles.
pub fn installed_in_system(ca_pem_body: &str) -> bool {
    let norm = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let needle = norm(ca_pem_body);
    if needle.len() < 100 {
        return false;
    }
    [
        "/etc/ssl/certs/ca-certificates.crt",
        "/etc/pki/tls/certs/ca-bundle.crt",
        "/etc/ca-certificates/extracted/tls-ca-bundle.pem",
        "/etc/ssl/ca-bundle.pem",
    ]
    .iter()
    .filter_map(|p| std::fs::read_to_string(p).ok())
    .any(|b| norm(&b).contains(&needle))
}

/// Strip the PEM armor to the base64 body.
pub fn pem_body(pem: &str) -> String {
    pem.lines()
        .filter(|l| !l.starts_with("-----") && !l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("")
}

pub fn installed_in_nss() -> Option<bool> {
    let certutil = which("certutil")?;
    let home = std::env::var("HOME").ok()?;
    let db = Path::new(&home).join(".pki/nssdb");
    if !db.exists() {
        return Some(false);
    }
    let out = Command::new(certutil)
        .args(["-d", &format!("sql:{}", db.display()), "-L"])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).contains("neko-ca"))
}

#[derive(Debug, Clone)]
pub struct CommandResult {
    pub ok: bool,
    pub output: String,
}

/// Run a shell command as root through polkit's graphical prompt.
pub fn run_privileged(script: &str) -> CommandResult {
    let out = Command::new("pkexec").args(["sh", "-c", script]).output();
    match out {
        Ok(o) => CommandResult {
            ok: o.status.success(),
            output: format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            )
            .trim()
            .to_string(),
        },
        Err(e) => CommandResult {
            ok: false,
            output: format!("Could not run pkexec: {e}"),
        },
    }
}

pub fn run_plain(script: &str) -> CommandResult {
    match Command::new("sh").args(["-c", script]).output() {
        Ok(o) => CommandResult {
            ok: o.status.success(),
            output: format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            )
            .trim()
            .to_string(),
        },
        Err(e) => CommandResult {
            ok: false,
            output: e.to_string(),
        },
    }
}

// ------------------------------------------------------------ browsers

#[derive(Clone, Debug)]
pub struct Browser {
    pub name: &'static str,
    pub exe: PathBuf,
    pub firefox: bool,
}

pub fn detect_browsers() -> Vec<Browser> {
    let candidates: [(&str, &[&str], bool); 6] = [
        ("Chromium", &["chromium", "chromium-browser"], false),
        (
            "Google Chrome",
            &["google-chrome", "google-chrome-stable"],
            false,
        ),
        (
            "Microsoft Edge",
            &["microsoft-edge", "microsoft-edge-stable"],
            false,
        ),
        ("Brave", &["brave-browser", "brave"], false),
        ("Vivaldi", &["vivaldi", "vivaldi-stable"], false),
        ("Firefox", &["firefox"], true),
    ];
    candidates
        .iter()
        .filter_map(|(name, bins, firefox)| {
            bins.iter().find_map(|b| which(b)).map(|exe| Browser {
                name,
                exe,
                firefox: *firefox,
            })
        })
        .collect()
}

pub struct LaunchOptions<'a> {
    pub proxy: &'a str,
    pub ca_path: &'a str,
    /// Base64 SHA-256 of the CA public key, for the zero-install flag.
    pub spki: Option<&'a str>,
}

/// Start a browser through the proxy with a throwaway profile. Returns a short
/// description of what was done.
pub fn launch(browser: &Browser, o: &LaunchOptions) -> Result<String, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("neko-browser-{stamp}"));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut cmd = Command::new(&browser.exe);
    let mut note = String::new();
    if browser.firefox {
        let (host, port) = o.proxy.rsplit_once(':').unwrap_or((o.proxy, "8899"));
        let prefs = format!(
            "user_pref(\"network.proxy.type\", 1);\n\
             user_pref(\"network.proxy.http\", \"{host}\");\n\
             user_pref(\"network.proxy.http_port\", {port});\n\
             user_pref(\"network.proxy.ssl\", \"{host}\");\n\
             user_pref(\"network.proxy.ssl_port\", {port});\n\
             user_pref(\"network.proxy.no_proxies_on\", \"\");\n\
             user_pref(\"network.proxy.allow_hijacking_localhost\", true);\n\
             user_pref(\"security.enterprise_roots.enabled\", true);\n\
             user_pref(\"browser.shell.checkDefaultBrowser\", false);\n\
             user_pref(\"datareporting.policy.dataSubmissionEnabled\", false);\n"
        );
        std::fs::write(dir.join("user.js"), prefs).map_err(|e| e.to_string())?;
        if let Some(certutil) = which("certutil") {
            let db = format!("sql:{}", dir.display());
            let init = Command::new(&certutil)
                .args(["-N", "-d", &db, "--empty-password"])
                .stderr(Stdio::null())
                .status();
            if init.map(|s| s.success()).unwrap_or(false) {
                let _ = Command::new(&certutil)
                    .args([
                        "-A", "-d", &db, "-t", "CT,,", "-n", "neko-ca", "-i", o.ca_path,
                    ])
                    .stderr(Stdio::null())
                    .status();
                note = "The proxy certificate was added to this profile.".into();
            }
        }
        cmd.args(["--no-remote", "--profile"])
            .arg(&dir)
            .arg("about:blank");
    } else {
        cmd.arg(format!("--proxy-server={}", o.proxy))
            .arg("--proxy-bypass-list=<-loopback>")
            .arg(format!("--user-data-dir={}", dir.display()))
            .arg("--no-first-run")
            .arg("--no-default-browser-check");
        if let Some(spki) = o.spki {
            cmd.arg(format!("--ignore-certificate-errors-spki-list={spki}"));
            note = "Trusting the proxy certificate in this window only.".into();
        }
        cmd.arg("about:blank");
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not start {}: {e}", browser.name))?;
    Ok(if note.is_empty() {
        format!("Started {} through the proxy.", browser.name)
    } else {
        format!("Started {} through the proxy. {note}", browser.name)
    })
}

/// Variables that point common tools at the proxy and its CA.
pub fn env_snippet(proxy: &str, ca_path: &str) -> String {
    let ca = sh_quote(ca_path);
    format!(
        "export HTTP_PROXY=http://{proxy}\n\
         export HTTPS_PROXY=http://{proxy}\n\
         export ALL_PROXY=http://{proxy}\n\
         export SSL_CERT_FILE={ca}\n\
         export CURL_CA_BUNDLE={ca}\n\
         export REQUESTS_CA_BUNDLE={ca}\n\
         export NODE_EXTRA_CA_CERTS={ca}"
    )
}

pub fn open_path(path: &str) {
    let _ = Command::new("xdg-open")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

// ------------------------------------------------------------ core location

/// Find `nekod`: explicit env, next to this executable, or the workspace
/// build output when running from a checkout.
pub fn find_nekod() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("NEKOD_PATH").map(PathBuf::from) {
        if p.is_file() {
            return Some(p);
        }
    }
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let mut candidates = vec![dir.join("nekod")];
    for up in dir.ancestors().take(5) {
        candidates.push(up.join("target/release/nekod"));
        candidates.push(up.join("target/debug/nekod"));
    }
    candidates
        .into_iter()
        .find(|p| p.is_file())
        .or_else(|| which("nekod"))
}

pub fn default_socket() -> PathBuf {
    if let Some(p) = std::env::var_os("NEKO_SOCKET") {
        return PathBuf::from(p);
    }
    match std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from) {
        Some(d) if d.is_absolute() => d.join("neko").join("api.sock"),
        _ => PathBuf::from(format!("/tmp/neko-{}/api.sock", unsafe { libc::getuid() })),
    }
}

// ------------------------------------------------------------ preferences

#[derive(Clone, Debug, PartialEq)]
pub struct Prefs {
    /// "system", "light" or "dark".
    pub theme: String,
    /// None follows the system.
    pub reduce_motion: Option<bool>,
    pub detail_width: f32,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            theme: "system".into(),
            reduce_motion: None,
            detail_width: 0.0,
        }
    }
}

fn prefs_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config")
        });
    base.join("neko").join("desktop.conf")
}

impl Prefs {
    pub fn load() -> Prefs {
        let mut p = Prefs::default();
        if let Ok(text) = std::fs::read_to_string(prefs_path()) {
            for line in text.lines() {
                let Some((k, v)) = line.split_once('=') else {
                    continue;
                };
                match k.trim() {
                    "theme" => p.theme = v.trim().to_string(),
                    "reduce_motion" => {
                        p.reduce_motion = match v.trim() {
                            "true" => Some(true),
                            "false" => Some(false),
                            _ => None,
                        }
                    }
                    "detail_width" => {
                        if let Ok(w) = v.trim().parse::<f32>() {
                            p.detail_width = if w > 0.0 { w.clamp(300.0, 1400.0) } else { 0.0 };
                        }
                    }
                    _ => {}
                }
            }
        }
        p
    }

    pub fn save(&self) {
        let path = prefs_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let rm = match self.reduce_motion {
            Some(true) => "true",
            Some(false) => "false",
            None => "system",
        };
        let _ = std::fs::write(
            path,
            format!(
                "theme={}\nreduce_motion={rm}\ndetail_width={:.0}\n",
                self.theme, self.detail_width
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_use_the_documented_commands() {
        let p = system_plan(Family::Fedora, "/home/u/.local/share/neko/ca.pem");
        assert!(p.display.contains("update-ca-trust"));
        assert!(p.display.starts_with("sudo cp"));
        assert!(!p.run.unwrap().contains("sudo"));
        let p = system_plan(Family::Debian, "/x/ca.pem");
        assert!(p.display.contains("update-ca-certificates"));
        let p = system_plan(Family::Arch, "/x/ca.pem");
        assert!(p.display.contains("trust anchor --store"));
        assert!(system_plan(Family::Unknown, "/x/ca.pem").run.is_none());
    }

    #[test]
    fn quoting_survives_apostrophes() {
        assert_eq!(sh_quote("a'b"), "'a'\\''b'");
    }

    #[test]
    fn env_snippet_covers_the_documented_variables() {
        let s = env_snippet("127.0.0.1:8899", "/x/ca.pem");
        for v in [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "SSL_CERT_FILE",
            "CURL_CA_BUNDLE",
            "NODE_EXTRA_CA_CERTS",
        ] {
            assert!(s.contains(v), "{v}");
        }
    }

    #[test]
    fn pem_body_strips_armor() {
        assert_eq!(
            pem_body("-----BEGIN CERTIFICATE-----\nAAA\nBBB\n-----END CERTIFICATE-----\n"),
            "AAABBB"
        );
    }
}
