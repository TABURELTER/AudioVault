//! Human-readable formatting helpers (sizes, durations, timestamps).

use std::time::{SystemTime, UNIX_EPOCH};

/// Current Unix time in seconds.
pub fn now_ts() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

pub fn bytes(b: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB", "PB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{b} B") } else if v >= 100.0 { format!("{v:.0} {}", UNITS[i]) } else { format!("{v:.1} {}", UNITS[i]) }
}

pub fn duration_ms(ms: u64) -> String {
    let total = ms / 1000;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 { format!("{h} h {m:02} min") } else if m > 0 { format!("{m} min {s:02} s") } else { format!("{s} s") }
}

pub fn rate(bytes_per_sec: f64) -> String {
    format!("{}/s", bytes(bytes_per_sec.max(0.0) as u64))
}

pub fn eta(secs: Option<u64>) -> String {
    match secs {
        None => "—".into(),
        Some(s) if s > 86400 * 7 => "> 1 week".into(),
        Some(s) => duration_ms(s * 1000),
    }
}

/// "5 min ago", "3 days ago", "never".
pub fn relative_time(ts: Option<i64>, now: i64) -> String {
    let Some(ts) = ts.filter(|t| *t > 0) else { return "never".into() };
    let d = now - ts;
    if d < 0 {
        return "just now".into();
    }
    match d {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", d / 60),
        3600..=86399 => format!("{} h ago", d / 3600),
        86400..=2_591_999 => format!("{} days ago", d / 86400),
        _ => format!("{} months ago", d / 2_592_000),
    }
}

/// UTC date "YYYY-MM-DD HH:MM" without pulling a date crate.
pub fn datetime(ts: i64) -> String {
    let days = ts.div_euclid(86400);
    let secs = ts.rem_euclid(86400);
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", secs / 3600, (secs % 3600) / 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(2_400_000_000), "2.2 GB");
        assert_eq!(duration_ms(16 * 3600 * 1000 + 5 * 60_000), "16 h 05 min");
        assert_eq!(datetime(0), "1970-01-01 00:00");
        assert_eq!(datetime(1_700_000_000), "2023-11-14 22:13");
        assert_eq!(relative_time(Some(100), 100 + 7200), "2 h ago");
        assert_eq!(relative_time(None, 1), "never");
    }
}
