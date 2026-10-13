//! Best-effort "which local app opened this connection" lookup, from
//! /proc/net/tcp and /proc/*/fd. Runs off the data path.

use std::collections::VecDeque;
use std::fs;
use std::sync::Mutex;

#[derive(Default)]
pub struct AppResolver {
    recent: Mutex<VecDeque<u32>>,
}

impl AppResolver {
    /// `client_port` is the client socket's local port, `proxy_port` the
    /// port it connected to.
    pub fn resolve(&self, client_port: u16, proxy_port: u16) -> Option<String> {
        let inode = socket_inode(client_port, proxy_port)?;
        let needle = format!("socket:[{inode}]");

        let recent: Vec<u32> = self.recent.lock().unwrap().iter().copied().collect();
        for pid in recent {
            if owns(pid, &needle) {
                return Some(name_of(pid));
            }
        }
        // Short-lived clients are the ones that race this scan, and they are
        // the newest processes: look at the highest pids first.
        let mut pids: Vec<u32> = fs::read_dir("/proc")
            .ok()?
            .flatten()
            .filter_map(|e| e.file_name().to_str().and_then(|s| s.parse().ok()))
            .collect();
        pids.sort_unstable_by(|a, b| b.cmp(a));
        for pid in pids {
            if owns(pid, &needle) {
                let mut r = self.recent.lock().unwrap();
                r.retain(|p| *p != pid);
                r.push_front(pid);
                r.truncate(8);
                return Some(name_of(pid));
            }
        }
        None
    }
}

fn socket_inode(client_port: u16, proxy_port: u16) -> Option<u64> {
    for file in ["/proc/net/tcp", "/proc/net/tcp6"] {
        let Ok(text) = fs::read_to_string(file) else {
            continue;
        };
        for line in text.lines().skip(1) {
            let mut cols = line.split_whitespace();
            let (_sl, local, remote) = (cols.next()?, cols.next()?, cols.next()?);
            let lp = local
                .rsplit(':')
                .next()
                .and_then(|p| u16::from_str_radix(p, 16).ok());
            let rp = remote
                .rsplit(':')
                .next()
                .and_then(|p| u16::from_str_radix(p, 16).ok());
            if lp == Some(client_port) && rp == Some(proxy_port) {
                // Columns: st tx:rx tr:when retrnsmt uid timeout inode
                return line.split_whitespace().nth(9)?.parse().ok();
            }
        }
    }
    None
}

fn owns(pid: u32, needle: &str) -> bool {
    let Ok(dir) = fs::read_dir(format!("/proc/{pid}/fd")) else {
        return false;
    };
    dir.flatten().any(|e| {
        fs::read_link(e.path())
            .map(|t| t.to_string_lossy() == needle)
            .unwrap_or(false)
    })
}

fn name_of(pid: u32) -> String {
    if let Ok(exe) = fs::read_link(format!("/proc/{pid}/exe")) {
        let s = exe.to_string_lossy();
        let s = s.trim_end_matches(" (deleted)");
        if let Some(name) = s.rsplit('/').next().filter(|n| !n.is_empty()) {
            return name.to_string();
        }
    }
    fs::read_to_string(format!("/proc/{pid}/comm"))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| format!("pid {pid}"))
}
