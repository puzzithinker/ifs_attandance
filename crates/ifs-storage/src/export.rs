//! CSV export (UTF-8 BOM) and default filenames.

use crate::repository::StorageError;
use chrono::{DateTime, Datelike, Local};
use ifs_core::{MasterAgentRow, MasterStatus, VisitSnapshot};
use std::fs::File;
use std::io::Write;
use std::path::Path;

const ENGLISH_MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Sanitize event name for use in filenames (ASCII-ish slug; empty if blank).
pub fn sanitize_event_slug(event_name: &str) -> String {
    let t = event_name.trim();
    if t.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for ch in t.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else if ch == '-' || ch == '_' {
            out.push(ch);
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        } else if out.is_empty() && ch.is_whitespace() {
            // skip leading
        } else if ch.is_whitespace() || !ch.is_ascii() {
            // non-ascii: keep as unicode letter if alphanumeric unicode
            if ch.is_alphanumeric() {
                out.push(ch);
            } else if !out.ends_with('_') && !out.is_empty() {
                out.push('_');
            }
        }
    }
    let out = out.trim_matches('_').to_string();
    // limit length for filesystems
    if out.chars().count() > 40 {
        out.chars().take(40).collect()
    } else {
        out
    }
}

/// Desk CSV default filename; includes event slug when set.
pub fn default_export_filename(now: DateTime<Local>, event_name: &str) -> String {
    let day = now.day();
    let month = ENGLISH_MONTHS[(now.month0()) as usize];
    let slug = sanitize_event_slug(event_name);
    if slug.is_empty() {
        format!("IFS_attendance_{day:02}-{month}.csv")
    } else {
        format!("IFS_attendance_{slug}_{day:02}-{month}.csv")
    }
}

/// Master CSV filename with optional event slug.
pub fn master_export_filename(now: DateTime<Local>, event_name: &str) -> String {
    let date = now.format("%Y-%m-%d");
    let slug = sanitize_event_slug(event_name);
    if slug.is_empty() {
        format!("IFS_master_attendance_{date}.csv")
    } else {
        format!("IFS_master_attendance_{slug}_{date}.csv")
    }
}

/// Write desk/detail visits CSV; returns row count (excluding header).
/// Includes an `event` column so exports identify the event when set.
pub fn write_visits_csv(
    path: &Path,
    visits: &[VisitSnapshot],
    event_name: &str,
) -> Result<u64, StorageError> {
    let mut f = File::create(path)?;
    f.write_all(&[0xEF, 0xBB, 0xBF])?;
    writeln!(
        f,
        "event,ID,保險中介人類別,保險中介人編號,入場時間,離場時間,station_id,visit_uid"
    )?;
    let event = event_name.trim();
    for (i, v) in visits.iter().enumerate() {
        let out = v.check_out_at.as_deref().unwrap_or("");
        writeln!(
            f,
            "{},{},{},{},{},{},{},{}",
            csv_escape(event),
            i + 1,
            csv_escape(&v.identity.category),
            csv_escape(&v.identity.license_no),
            csv_escape(&v.check_in_at),
            csv_escape(out),
            csv_escape(&v.station_id),
            csv_escape(&v.visit_uid),
        )?;
    }
    Ok(visits.len() as u64)
}

/// Master rollup CSV (event column first when identifying multi-event archives).
pub fn export_master_csv(
    path: &Path,
    rows: &[MasterAgentRow],
    event_name: &str,
) -> Result<u64, StorageError> {
    let mut f = File::create(path)?;
    f.write_all(&[0xEF, 0xBB, 0xBF])?;
    writeln!(
        f,
        "event,保險中介人類別,保險中介人編號,first_check_in_at,last_check_out_at,stations_seen,visit_count,open_stations,status,needs_review"
    )?;
    let event = event_name.trim();
    for r in rows {
        let stations = r.stations_seen.join("|");
        let open = r.open_stations.join("|");
        let status = match r.status {
            MasterStatus::Left => "已離場",
            MasterStatus::StillInside => "仍在場",
            MasterStatus::CheckInOnly => "僅入場",
            MasterStatus::OrphanOutOnly => "僅離場(無入場)",
        };
        writeln!(
            f,
            "{},{},{},{},{},{},{},{},{},{}",
            csv_escape(event),
            csv_escape(&r.identity.category),
            csv_escape(&r.identity.license_no),
            csv_escape(r.first_check_in_at.as_deref().unwrap_or("")),
            csv_escape(r.last_check_out_at.as_deref().unwrap_or("")),
            csv_escape(&stations),
            r.visit_count,
            csv_escape(&open),
            status,
            if r.needs_review { "1" } else { "0" },
        )?;
    }
    Ok(rows.len() as u64)
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use ifs_core::{AgentIdentity, VisitSnapshot};
    use tempfile::tempdir;

    #[test]
    fn english_month_filename() {
        let dt = Local.with_ymd_and_hms(2026, 8, 7, 12, 0, 0).unwrap();
        assert_eq!(
            default_export_filename(dt, ""),
            "IFS_attendance_07-August.csv"
        );
        let with_event = default_export_filename(dt, "CPD 下午");
        assert!(with_event.starts_with("IFS_attendance_"));
        assert!(with_event.contains("CPD"));
        assert!(with_event.ends_with("07-August.csv"));
        let dt = Local.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        assert_eq!(
            default_export_filename(dt, ""),
            "IFS_attendance_01-January.csv"
        );
    }

    #[test]
    fn sanitize_slug_basic() {
        assert_eq!(sanitize_event_slug(""), "");
        assert_eq!(sanitize_event_slug("  "), "");
        assert!(sanitize_event_slug("Hello World").contains("Hello"));
        assert!(!sanitize_event_slug("a/b\\c").contains('/'));
    }

    #[test]
    fn master_filename_with_event() {
        let dt = Local.with_ymd_and_hms(2026, 8, 9, 12, 0, 0).unwrap();
        assert_eq!(
            master_export_filename(dt, ""),
            "IFS_master_attendance_2026-08-09.csv"
        );
        assert!(master_export_filename(dt, "Event1").contains("Event1"));
    }

    #[test]
    fn write_visits_csv_includes_event_column() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.csv");
        let visits = [VisitSnapshot {
            identity: AgentIdentity::new("IA", "1"),
            check_in_at: "2026-08-08T09:00:00".into(),
            check_out_at: None,
            station_id: "S1".into(),
            visit_uid: "u1".into(),
        }];
        let n = write_visits_csv(&path, &visits, "測試活動").unwrap();
        assert_eq!(n, 1);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("event,ID,"));
        assert!(text.contains("測試活動"));
        assert!(text.contains("2026-08-08T09:00:00"));
    }
}
