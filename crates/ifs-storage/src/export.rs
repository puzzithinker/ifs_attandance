//! CSV export (UTF-8 BOM) and default filenames.

use crate::repository::StorageError;
use chrono::{DateTime, Datelike, Local};
use ifs_core::{CpdDecision, CpdPolicy, MasterAgentRow, MasterStatus, VisitSnapshot};
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

/// CPD column text for one visit: points earned, `0` when missed, blank when
/// no policy is configured for this event.
fn cpd_column(policy: Option<&CpdPolicy>, check_in_at: &str, check_out_at: Option<&str>) -> String {
    match policy {
        None => String::new(),
        Some(p) => match p.decide(check_in_at, check_out_at) {
            CpdDecision::Earned => p.points.to_string(),
            CpdDecision::Missed => "0".to_string(),
        },
    }
}

/// Write desk/detail visits CSV; returns row count (excluding header).
/// Includes an `event` column so exports identify the event when set.
pub fn write_visits_csv(
    path: &Path,
    visits: &[VisitSnapshot],
    event_name: &str,
    policy: Option<&CpdPolicy>,
) -> Result<u64, StorageError> {
    let mut f = File::create(path)?;
    f.write_all(&[0xEF, 0xBB, 0xBF])?;
    writeln!(
        f,
        "event,ID,保險中介人類別,保險中介人編號,入場時間,離場時間,CPD"
    )?;
    let event = event_name.trim();
    for (i, v) in visits.iter().enumerate() {
        let out = v.check_out_at.as_deref().unwrap_or("");
        writeln!(
            f,
            "{},{},{},{},{},{},{}",
            csv_escape(event),
            i + 1,
            csv_escape(&v.identity.category),
            csv_escape(&v.identity.license_no),
            csv_escape(&v.check_in_at),
            csv_escape(out),
            cpd_column(policy, &v.check_in_at, v.check_out_at.as_deref()),
        )?;
    }
    Ok(visits.len() as u64)
}

/// Master rollup CSV (event column first when identifying multi-event archives).
pub fn export_master_csv(
    path: &Path,
    rows: &[MasterAgentRow],
    event_name: &str,
    policy: Option<&CpdPolicy>,
) -> Result<u64, StorageError> {
    let mut f = File::create(path)?;
    f.write_all(&[0xEF, 0xBB, 0xBF])?;
    writeln!(
        f,
        "event,保險中介人類別,保險中介人編號,first_check_in_at,last_check_out_at,stations_seen,visit_count,open_stations,status,CPD,needs_review"
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
            "{},{},{},{},{},{},{},{},{},{},{}",
            csv_escape(event),
            csv_escape(&r.identity.category),
            csv_escape(&r.identity.license_no),
            csv_escape(r.first_check_in_at.as_deref().unwrap_or("")),
            csv_escape(r.last_check_out_at.as_deref().unwrap_or("")),
            csv_escape(&stations),
            r.visit_count,
            csv_escape(&open),
            status,
            cpd_column(
                policy,
                r.first_check_in_at.as_deref().unwrap_or(""),
                r.last_check_out_at.as_deref()
            ),
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
    fn write_visits_csv_columns_and_blank_cpd_without_policy() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.csv");
        let visits = [VisitSnapshot {
            identity: AgentIdentity::new("IA", "1"),
            check_in_at: "2026-08-08T09:00:00".into(),
            check_out_at: None,
            station_id: "S1".into(),
            visit_uid: "u1".into(),
        }];
        let n = write_visits_csv(&path, &visits, "測試活動", None).unwrap();
        assert_eq!(n, 1);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("event,ID,"));
        assert!(text.contains("測試活動"));
        assert!(text.contains("2026-08-08T09:00:00"));
        // station_id and visit_uid dropped; CPD present but blank without a
        // policy, so this row ends with two empty trailing fields.
        assert!(!text.contains("station_id"));
        assert!(!text.contains("visit_uid"));
        assert!(text.contains("入場時間,離場時間,CPD\n"));
        assert!(text.contains(",2026-08-08T09:00:00,,\n"));
    }

    #[test]
    fn write_visits_csv_cpd_points_zero_and_blank() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.csv");
        let visit = |lic: &str, cin: &str, cout: Option<&str>| VisitSnapshot {
            identity: AgentIdentity::new("IA", lic),
            check_in_at: cin.into(),
            check_out_at: cout.map(str::to_string),
            station_id: "S1".into(),
            visit_uid: format!("u-{lic}"),
        };
        let visits = [
            // Full session inside both windows → earned.
            visit("A", "2026-09-10T14:30:00", Some("2026-09-10T17:30:00")),
            // Left too early → missed.
            visit("B", "2026-09-10T14:40:00", Some("2026-09-10T17:09:00")),
            // Still inside at export time → missed.
            visit("C", "2026-09-10T14:50:00", None),
        ];
        let policy = CpdPolicy::from_config("14:30", "15:00", "17:10", "17:30", "2")
            .unwrap()
            .unwrap();
        write_visits_csv(&path, &visits, "", Some(&policy)).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(",IA,A,2026-09-10T14:30:00,2026-09-10T17:30:00,2\n"));
        assert!(text.contains(",IA,B,2026-09-10T14:40:00,2026-09-10T17:09:00,0\n"));
        assert!(text.contains(",IA,C,2026-09-10T14:50:00,,0\n"));
    }

    #[test]
    fn master_csv_cpd_uses_first_in_and_last_out() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("m.csv");
        let row = MasterAgentRow {
            identity: AgentIdentity::new("IA", "1"),
            first_check_in_at: Some("2026-09-10T14:45:00".into()),
            last_check_out_at: Some("2026-09-10T17:20:00".into()),
            stations_seen: vec!["S1".into()],
            visit_count: 1,
            open_stations: vec![],
            status: MasterStatus::Left,
            needs_review: false,
        };
        let policy = CpdPolicy::from_config("14:30", "15:00", "17:10", "17:30", "2")
            .unwrap()
            .unwrap();
        let n = export_master_csv(&path, &[row], "", Some(&policy)).unwrap();
        assert_eq!(n, 1);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("status,CPD,needs_review"));
        assert!(text.contains("已離場,2,0"));
    }
}
