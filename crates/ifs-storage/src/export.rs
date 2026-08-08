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

/// `IFS_AML_seminar_attendance_%d-%B.csv` with fixed English month names.
pub fn default_export_filename(now: DateTime<Local>) -> String {
    let day = now.day();
    let month = ENGLISH_MONTHS[(now.month0()) as usize];
    format!("IFS_AML_seminar_attendance_{day:02}-{month}.csv")
}

/// Write desk/detail visits CSV; returns row count (excluding header).
pub fn write_visits_csv(path: &Path, visits: &[VisitSnapshot]) -> Result<u64, StorageError> {
    let mut f = File::create(path)?;
    // UTF-8 BOM
    f.write_all(&[0xEF, 0xBB, 0xBF])?;
    writeln!(
        f,
        "ID,保險中介人類別,保險中介人編號,入場時間,離場時間,station_id,visit_uid"
    )?;
    for (i, v) in visits.iter().enumerate() {
        let out = v.check_out_at.as_deref().unwrap_or("");
        writeln!(
            f,
            "{},{},{},{},{},{},{}",
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

/// Master rollup CSV.
pub fn export_master_csv(path: &Path, rows: &[MasterAgentRow]) -> Result<u64, StorageError> {
    let mut f = File::create(path)?;
    f.write_all(&[0xEF, 0xBB, 0xBF])?;
    writeln!(
        f,
        "保險中介人類別,保險中介人編號,first_check_in_at,last_check_out_at,stations_seen,visit_count,open_stations,status,needs_review"
    )?;
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
            "{},{},{},{},{},{},{},{},{}",
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
            default_export_filename(dt),
            "IFS_AML_seminar_attendance_07-August.csv"
        );
        let dt = Local.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        assert_eq!(
            default_export_filename(dt),
            "IFS_AML_seminar_attendance_01-January.csv"
        );
        let dt = Local.with_ymd_and_hms(2026, 12, 31, 23, 0, 0).unwrap();
        assert_eq!(
            default_export_filename(dt),
            "IFS_AML_seminar_attendance_31-December.csv"
        );
    }

    #[test]
    fn write_visits_csv_empty_checkout_column() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.csv");
        let visits = [VisitSnapshot {
            identity: AgentIdentity::new("IA", "1"),
            check_in_at: "2026-08-08T09:00:00".into(),
            check_out_at: None,
            station_id: "S1".into(),
            visit_uid: "u1".into(),
        }];
        let n = write_visits_csv(&path, &visits).unwrap();
        assert_eq!(n, 1);
        let text = std::fs::read_to_string(&path).unwrap();
        // BOM may make starts_with awkward; check header and open checkout (empty field)
        assert!(text.contains("入場時間"));
        assert!(text.contains("2026-08-08T09:00:00"));
        // row ends with station and uid; empty checkout is consecutive commas region
        assert!(text.contains("2026-08-08T09:00:00,,S1,u1") || text.contains("09:00:00,,S1"));
    }
}
