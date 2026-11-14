//! Local time for logs and backup names.

use std::time::{SystemTime, UNIX_EPOCH};

struct Tm {
    year: i32,
    month: i32,
    day: i32,
    hour: i32,
    min: i32,
    sec: i32,
}

fn local(t: SystemTime) -> Tm {
    let secs = t.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as libc::time_t).unwrap_or(0);
    // SAFETY: localtime_r only writes the struct we hand it.
    let tm = unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&secs, &mut tm);
        tm
    };
    Tm { year: tm.tm_year + 1900, month: tm.tm_mon + 1, day: tm.tm_mday, hour: tm.tm_hour, min: tm.tm_min, sec: tm.tm_sec }
}

/// 2026-11-14 12:34:56
pub fn now_text() -> String {
    let t = local(SystemTime::now());
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", t.year, t.month, t.day, t.hour, t.min, t.sec)
}

/// 20261114-123456, for file names
pub fn now_stamp() -> String {
    let t = local(SystemTime::now());
    format!("{:04}{:02}{:02}-{:02}{:02}{:02}", t.year, t.month, t.day, t.hour, t.min, t.sec)
}
