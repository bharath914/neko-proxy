//! Display formatting. No parsing: values arrive typed from the core.

pub fn size(b: u64) -> String {
    match b {
        0 => "0 B".into(),
        b if b < 1024 => format!("{b} B"),
        b if b < 10 * 1024 => format!("{:.1} KB", b as f64 / 1024.0),
        b if b < 1024 * 1024 => format!("{:.0} KB", b as f64 / 1024.0),
        b if b < 10 * 1024 * 1024 => format!("{:.1} MB", b as f64 / 1048576.0),
        b => format!("{:.0} MB", b as f64 / 1048576.0),
    }
}

pub fn duration(ms: f64) -> String {
    if ms <= 0.0 {
        "—".into()
    } else if ms < 1.0 {
        format!("{:.0} µs", ms * 1000.0)
    } else if ms < 1000.0 {
        format!("{ms:.0} ms")
    } else if ms < 60_000.0 {
        format!("{:.2} s", ms / 1000.0)
    } else {
        format!("{:.1} min", ms / 60_000.0)
    }
}

pub fn ms_precise(ms: f64) -> String {
    if ms <= 0.0 {
        "0 ms".into()
    } else if ms < 10.0 {
        format!("{ms:.2} ms")
    } else if ms < 1000.0 {
        format!("{ms:.1} ms")
    } else {
        format!("{:.2} s", ms / 1000.0)
    }
}

pub fn count(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}

pub fn status_text(code: u32) -> &'static str {
    match code {
        101 => "Switching Protocols",
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        206 => "Partial Content",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        409 => "Conflict",
        410 => "Gone",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "",
    }
}

/// "just now", "12 s ago", "3 min ago", "2 h ago", "4 d ago".
pub fn ago(unix_ms: i64, now_ms: i64) -> String {
    if unix_ms <= 0 {
        return "never".into();
    }
    let s = ((now_ms - unix_ms) / 1000).max(0);
    match s {
        0..=4 => "just now".into(),
        5..=59 => format!("{s} s ago"),
        60..=3599 => format!("{} min ago", s / 60),
        3600..=86_399 => format!("{} h ago", s / 3600),
        _ => format!("{} d ago", s / 86_400),
    }
}

/// `482913` as `482 913`.
pub fn pair_code(code: &str) -> String {
    if code.len() == 6 && code.is_ascii() {
        format!("{} {}", &code[..3], &code[3..])
    } else {
        code.to_string()
    }
}

/// Local wall-clock time `HH:MM:SS.mmm` for a unix-ms timestamp.
pub fn clock(unix_ms: i64) -> String {
    let secs = (unix_ms.div_euclid(1000)) as libc::time_t;
    let ms = unix_ms.rem_euclid(1000);
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: localtime_r writes into `tm`, which is valid for the call.
    let ok = unsafe { !libc::localtime_r(&secs, &mut tm).is_null() };
    if !ok {
        return String::new();
    }
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        tm.tm_hour, tm.tm_min, tm.tm_sec, ms
    )
}

pub fn date_clock(unix_ms: i64) -> String {
    let secs = (unix_ms.div_euclid(1000)) as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let ok = unsafe { !libc::localtime_r(&secs, &mut tm).is_null() };
    if !ok {
        return String::new();
    }
    format!(
        "{:04}-{:02}-{:02} {}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        clock(unix_ms)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_durations() {
        assert_eq!(size(0), "0 B");
        assert_eq!(size(1536), "1.5 KB");
        assert_eq!(size(94_210), "92 KB");
        assert_eq!(duration(84.4), "84 ms");
        assert_eq!(duration(1840.0), "1.84 s");
        assert_eq!(duration(0.0), "—");
        assert_eq!(count(1204), "1,204");
        assert_eq!(count(12), "12");
        assert_eq!(ago(0, 5), "never");
        assert_eq!(ago(1_000, 1_000), "just now");
        assert_eq!(ago(1_000, 31_000), "30 s ago");
        assert_eq!(ago(1_000, 301_000), "5 min ago");
        assert_eq!(ago(1_000, 7_201_000), "2 h ago");
        assert_eq!(pair_code("482913"), "482 913");
        assert_eq!(pair_code("12"), "12");
    }
}
