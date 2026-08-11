//! Pure UI formatters (no egui) — unit-tested.

use std::time::{Duration, SystemTime};

/// Format elapsed wall time as human Chinese/English hybrid age text.
pub fn format_seconds_ago(secs: u64) -> String {
    if secs == 0 {
        "剛剛".to_string()
    } else if secs < 60 {
        format!("{secs} 秒前")
    } else if secs < 3600 {
        let m = secs / 60;
        let s = secs % 60;
        if s == 0 {
            format!("{m} 分鐘前")
        } else {
            format!("{m} 分 {s} 秒前")
        }
    } else {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        format!("{h} 小時 {m} 分鐘前")
    }
}

/// Seconds between `then` and `now` (clamped; 0 if then is in the future).
pub fn elapsed_secs(then: SystemTime, now: SystemTime) -> u64 {
    now.duration_since(then).unwrap_or(Duration::ZERO).as_secs()
}

/// Clipboard / copy payload for a recent scan identity.
/// Prefer license alone when present; else category·license.
pub fn format_copy_identity(category: &str, license_no: &str) -> String {
    let c = category.trim();
    let l = license_no.trim();
    if !l.is_empty() && !c.is_empty() {
        format!("{c} · {l}")
    } else if !l.is_empty() {
        l.to_string()
    } else if !c.is_empty() {
        c.to_string()
    } else {
        String::new()
    }
}

/// Parse "CAT · LIC" style subject line used in ScanFeedback.
pub fn parse_subject_identity(subject: &str) -> (String, String) {
    if let Some((a, b)) = subject.split_once("·") {
        (a.trim().to_string(), b.trim().to_string())
    } else if let Some((a, b)) = subject.split_once('/') {
        (a.trim().to_string(), b.trim().to_string())
    } else {
        (String::new(), subject.trim().to_string())
    }
}

/// Live clock display (local).
pub fn format_clock_now(now: chrono::DateTime<chrono::Local>) -> String {
    now.format("%Y-%m-%d %H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn seconds_ago_buckets() {
        assert_eq!(format_seconds_ago(0), "剛剛");
        assert_eq!(format_seconds_ago(3), "3 秒前");
        assert_eq!(format_seconds_ago(60), "1 分鐘前");
        assert_eq!(format_seconds_ago(65), "1 分 5 秒前");
        assert_eq!(format_seconds_ago(3600), "1 小時 0 分鐘前");
        assert_eq!(format_seconds_ago(3661), "1 小時 1 分鐘前");
    }

    #[test]
    fn elapsed_secs_ok() {
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        let t1 = SystemTime::UNIX_EPOCH + Duration::from_secs(1010);
        assert_eq!(elapsed_secs(t0, t1), 10);
        assert_eq!(elapsed_secs(t1, t0), 0);
    }

    #[test]
    fn copy_identity_formats() {
        assert_eq!(format_copy_identity("IA", "123"), "IA · 123");
        assert_eq!(format_copy_identity("", "123"), "123");
        assert_eq!(format_copy_identity("IA", ""), "IA");
        assert_eq!(format_copy_identity("  ", "  "), "");
    }

    #[test]
    fn parse_subject() {
        let (c, l) = parse_subject_identity("IA  ·  999");
        assert_eq!(c, "IA");
        assert_eq!(l, "999");
    }

    #[test]
    fn clock_format_contains_separators() {
        let s = format_clock_now(chrono::Local::now());
        assert!(s.contains('-') && s.contains(':'));
        assert_eq!(s.len(), 19);
    }
}
