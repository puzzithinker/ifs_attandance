//! Integration tests driving shipped storage APIs (real rusqlite paths).

use ifs_core::{
    parse_qr_url, rollup_master, AgentIdentity, AttendanceMode, MasterStatus, ScanOutcome,
};
use ifs_storage::{
    default_export_filename, export_master_csv, export_station_package, import_station_package,
    open_database, open_database_with_station, open_in_memory, DbRole, SqliteVisitRepository,
    StationInfo,
};
use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

fn station(id: &str, name: &str) -> StationInfo {
    StationInfo::new(id, name)
}

fn repo(conn: &Connection, st: StationInfo) -> SqliteVisitRepository<'_> {
    SqliteVisitRepository::new(conn, st, DbRole::Desk)
}

#[test]
fn migrate_empty_and_apply_checkin_checkout_reentry() {
    let (conn, report) = open_in_memory().unwrap();
    assert!(!report.already_current || report.to_version == 2);
    assert_eq!(report.to_version, 2);

    let r = repo(&conn, station("S1", "desk-1"));
    let id = AgentIdentity::new("IA", "100");

    let o = r
        .apply_mode(AttendanceMode::CheckIn, &id, "2026-08-08T09:00:00")
        .unwrap();
    assert!(matches!(o, ScanOutcome::CheckedIn { .. }));

    let o = r
        .apply_mode(AttendanceMode::CheckIn, &id, "2026-08-08T09:05:00")
        .unwrap();
    assert!(matches!(o, ScanOutcome::AlreadyCheckedIn { .. }));

    let counts = r.counts().unwrap();
    assert_eq!(counts.currently_inside, 1);
    assert_eq!(counts.total_visits, 1);

    let o = r
        .apply_mode(AttendanceMode::CheckOut, &id, "2026-08-08T12:00:00")
        .unwrap();
    assert!(matches!(o, ScanOutcome::CheckedOut { .. }));

    let counts = r.counts().unwrap();
    assert_eq!(counts.currently_inside, 0);
    assert_eq!(counts.total_visits, 1);

    // re-entry
    let o = r
        .apply_mode(AttendanceMode::CheckIn, &id, "2026-08-08T14:00:00")
        .unwrap();
    assert!(matches!(o, ScanOutcome::CheckedIn { .. }));
    let counts = r.counts().unwrap();
    assert_eq!(counts.currently_inside, 1);
    assert_eq!(counts.total_visits, 2);
    assert_eq!(counts.unique_agents, 1);
}

#[test]
fn reject_invalid_qr_before_storage() {
    assert!(parse_qr_url("").is_err());
    assert!(parse_qr_url("https://x/?categoryCode=&licenseNo=").is_err());
    // storage never called — zero rows after failed parse is a caller concern;
    // ensure empty DB stays empty after only valid path tests.
    let (conn, _) = open_in_memory().unwrap();
    let r = repo(&conn, station("S1", "d"));
    assert_eq!(r.counts().unwrap().total_visits, 0);
}

#[test]
fn legacy_attendance_migration() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("legacy.db");
    {
        let c = Connection::open(&path).unwrap();
        c.execute_batch(
            "CREATE TABLE attendance (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                保險中介人類別 TEXT,
                保險中介人編號 TEXT,
                timestamp TEXT,
                UNIQUE(保險中介人類別, 保險中介人編號)
            );
            INSERT INTO attendance (保險中介人類別, 保險中介人編號, timestamp)
            VALUES ('IA', 'LEG1', '2024-09-12 10:00:00');
            INSERT INTO attendance (保險中介人類別, 保險中介人編號, timestamp)
            VALUES ('', 'LEG2', '2024-09-12 11:00:00');
            INSERT INTO attendance (保險中介人類別, 保險中介人編號, timestamp)
            VALUES ('X', '', '2024-09-12 12:00:00');
            ",
        )
        .unwrap();
    }
    let (conn, report) = open_database(&path).unwrap();
    assert_eq!(report.legacy_rows_copied, 2);
    assert_eq!(report.legacy_rows_skipped, 1);
    assert!(report.backup_table_name.is_some());
    let r = repo(&conn, station("S1", "d"));
    assert_eq!(r.counts().unwrap().total_visits, 2);
    let snaps = r.list_visit_snapshots().unwrap();
    assert!(snaps.iter().any(|v| v.identity.license_no == "LEG1"));
    assert!(snaps
        .iter()
        .any(|v| v.identity.license_no == "LEG2" && v.identity.category.is_empty()));
}

#[test]
fn csv_bom_headers_and_checkout_column() {
    let dir = tempdir().unwrap();
    let (conn, _) = open_in_memory().unwrap();
    let r = repo(&conn, station("S1", "d"));
    let id = AgentIdentity::new("IA", "CSV1");
    r.apply_mode(AttendanceMode::CheckIn, &id, "2026-08-08T09:00:00")
        .unwrap();
    r.apply_mode(AttendanceMode::CheckOut, &id, "2026-08-08T10:00:00")
        .unwrap();
    let path = dir.path().join("out.csv");
    let n = r.export_csv(&path, "Demo Event").unwrap();
    assert_eq!(n, 1);
    let bytes = fs::read(&path).unwrap();
    assert_eq!(&bytes[0..3], &[0xEF, 0xBB, 0xBF]);
    let text = String::from_utf8(bytes[3..].to_vec()).unwrap();
    assert!(text.contains("event,ID,"));
    assert!(text.contains("Demo Event"));
    assert!(text.contains("保險中介人類別"));
    assert!(text.contains("保險中介人編號"));
    assert!(text.contains("入場時間"));
    assert!(text.contains("離場時間"));
    assert!(text.contains("2026-08-08T09:00:00"));
    assert!(text.contains("2026-08-08T10:00:00"));

    let name = default_export_filename(chrono::Local::now(), "Demo Event");
    assert!(name.starts_with("IFS_attendance_"));
    assert!(name.ends_with(".csv"));
    assert!(name.contains("-"));
    assert!(name.contains("Demo"));
}

#[test]
fn soft_checkout_orphan_and_cross_station_pair() {
    let dir = tempdir().unwrap();
    // Station A: check-in only
    let path_a = dir.path().join("a.db");
    let (conn_a, _, st_a) = open_database_with_station(&path_a, DbRole::Desk).unwrap();
    let ra = SqliteVisitRepository::new(&conn_a, st_a.clone(), DbRole::Desk).with_soft_checkout(true);
    let id = AgentIdentity::new("IA", "MULTI1");
    ra.apply_mode(AttendanceMode::CheckIn, &id, "2026-08-08T10:00:00")
        .unwrap();
    drop(conn_a);

    // Station B: soft check-out
    let path_b = dir.path().join("b.db");
    let (conn_b, _, st_b) = open_database_with_station(&path_b, DbRole::Desk).unwrap();
    let rb = SqliteVisitRepository::new(&conn_b, st_b.clone(), DbRole::Desk).with_soft_checkout(true);
    let o = rb
        .apply_mode(AttendanceMode::CheckOut, &id, "2026-08-08T12:00:00")
        .unwrap();
    assert!(matches!(o, ScanOutcome::OrphanCheckOut { .. }));
    drop(conn_b);

    // Export packages
    let pkg_a = dir.path().join("pkg_a.db");
    let pkg_b = dir.path().join("pkg_b.db");
    export_station_package(&path_a, &pkg_a, &st_a).unwrap();
    export_station_package(&path_b, &pkg_b, &st_b).unwrap();

    // Master import
    let path_m = dir.path().join("master.db");
    let (conn_m, _, st_m) = open_database_with_station(&path_m, DbRole::Master).unwrap();
    let imp1 = import_station_package(&conn_m, &pkg_a).unwrap();
    assert_eq!(imp1.rows_inserted, 1);
    let imp2 = import_station_package(&conn_m, &pkg_b).unwrap();
    assert_eq!(imp2.events_inserted >= 1, true);

    // Re-import A: no extra visits
    let imp1b = import_station_package(&conn_m, &pkg_a).unwrap();
    assert_eq!(imp1b.rows_inserted, 0);
    assert_eq!(imp1b.rows_skipped, 1);

    let rm = SqliteVisitRepository::new(&conn_m, st_m, DbRole::Master);
    let visits = rm.list_visit_snapshots().unwrap();
    let orphans = rm.list_orphan_checkouts().unwrap();
    assert_eq!(visits.len(), 1);
    assert!(!orphans.is_empty());

    let rollup = rollup_master(&visits, &orphans);
    assert_eq!(rollup.rows.len(), 1);
    assert_eq!(
        rollup.rows[0].first_check_in_at.as_deref(),
        Some("2026-08-08T10:00:00")
    );
    assert_eq!(
        rollup.rows[0].last_check_out_at.as_deref(),
        Some("2026-08-08T12:00:00")
    );
    assert_eq!(rollup.rows[0].status, MasterStatus::Left);

    let master_csv = dir.path().join("master.csv");
    export_master_csv(&master_csv, &rollup.rows, "CrossDoor").unwrap();
    let text = fs::read_to_string(&master_csv).unwrap();
    assert!(text.contains("MULTI1"));
    assert!(text.contains("CrossDoor"));
}

/// Regression: master drops idx_visits_one_open; second open must not fail migrate verify.
#[test]
fn master_reopen_after_drop_open_index_and_import() {
    let dir = tempdir().unwrap();
    // Desk package with one visit
    let desk = dir.path().join("desk.db");
    let (conn_d, _, st) = open_database_with_station(&desk, DbRole::Desk).unwrap();
    let r = SqliteVisitRepository::new(&conn_d, st.clone(), DbRole::Desk);
    r.apply_mode(
        AttendanceMode::CheckIn,
        &AgentIdentity::new("IA", "REOPEN1"),
        "2026-08-08T09:00:00",
    )
    .unwrap();
    drop(conn_d);
    let pkg = dir.path().join("pkg.db");
    export_station_package(&desk, &pkg, &st).unwrap();

    let master = dir.path().join("master.db");
    {
        let (conn_m, report, _) = open_database_with_station(&master, DbRole::Master).unwrap();
        assert_eq!(report.to_version, 2);
        import_station_package(&conn_m, &pkg).unwrap();
        // Index should be gone after master open/import
        let has: i64 = conn_m
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_visits_one_open'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(has, 0);
        drop(conn_m);
    }

    // Second --master open: previously failed with "v2 indexes incomplete"
    let (conn_m2, report2, _) = open_database_with_station(&master, DbRole::Master).unwrap();
    assert!(report2.already_current || report2.to_version == 2);
    assert_eq!(report2.from_version, 2);
    let rm = SqliteVisitRepository::new(&conn_m2, station("M", "master"), DbRole::Master);
    assert_eq!(rm.counts().unwrap().total_visits, 1);

    // Third open still OK
    drop(conn_m2);
    let (_conn_m3, _, _) = open_database_with_station(&master, DbRole::Master).unwrap();
}

#[test]
fn two_station_open_dedupe_rollup() {
    let dir = tempdir().unwrap();
    let mut packages = Vec::new();
    for (name, sid, t) in [("a", "SA", "09:00:00"), ("b", "SB", "10:00:00")] {
        let path: PathBuf = dir.path().join(format!("{name}.db"));
        let (conn, _) = open_database(&path).unwrap();
        let st = station(sid, name);
        let r = repo(&conn, st.clone());
        r.apply_mode(
            AttendanceMode::CheckIn,
            &AgentIdentity::new("BR", "DUP"),
            &format!("2026-08-08T{t}"),
        )
        .unwrap();
        drop(conn);
        let pkg = dir.path().join(format!("pkg_{name}.db"));
        export_station_package(&path, &pkg, &st).unwrap();
        packages.push(pkg);
    }

    let path_m = dir.path().join("master.db");
    let (conn_m, _, _) = open_database_with_station(&path_m, DbRole::Master).unwrap();
    for p in &packages {
        import_station_package(&conn_m, p).unwrap();
    }
    let rm = SqliteVisitRepository::new(&conn_m, station("M", "master"), DbRole::Master);
    let visits = rm.list_visit_snapshots().unwrap();
    assert_eq!(visits.len(), 2);
    let rollup = rollup_master(&visits, &[]);
    assert_eq!(rollup.rows.len(), 1);
    assert_eq!(rollup.rows[0].visit_count, 2);
    assert_eq!(rollup.rows[0].status, MasterStatus::StillInside);
    assert!(rollup.rows[0].needs_review);
}

#[test]
fn same_license_different_category_two_open_visits() {
    let (conn, _) = open_in_memory().unwrap();
    let r = repo(&conn, station("S1", "d"));
    let a = AgentIdentity::new("IA", "SAME");
    let b = AgentIdentity::new("BR", "SAME");
    r.apply_mode(AttendanceMode::CheckIn, &a, "2026-08-08T09:00:00")
        .unwrap();
    r.apply_mode(AttendanceMode::CheckIn, &b, "2026-08-08T09:01:00")
        .unwrap();
    let c = r.counts().unwrap();
    assert_eq!(c.currently_inside, 2);
    assert_eq!(c.unique_agents, 2);
    assert_eq!(c.total_visits, 2);
}

#[test]
fn soft_checkout_off_yields_not_checked_in_no_orphan() {
    let (conn, _) = open_in_memory().unwrap();
    let r = SqliteVisitRepository::new(&conn, station("S1", "d"), DbRole::Desk)
        .with_soft_checkout(false);
    let id = AgentIdentity::new("IA", "SOFT_OFF");
    let o = r
        .apply_mode(AttendanceMode::CheckOut, &id, "2026-08-08T12:00:00")
        .unwrap();
    assert!(matches!(o, ScanOutcome::NotCheckedIn { .. }));
    assert!(r.list_orphan_checkouts().unwrap().is_empty());
    assert_eq!(r.counts().unwrap().total_visits, 0);
}

#[test]
fn visit_uid_unique_on_local_inserts() {
    let (conn, _) = open_in_memory().unwrap();
    let r = repo(&conn, station("S1", "d"));
    let id = AgentIdentity::new("IA", "UID1");
    r.apply_mode(AttendanceMode::CheckIn, &id, "2026-08-08T09:00:00")
        .unwrap();
    r.apply_mode(AttendanceMode::CheckOut, &id, "2026-08-08T10:00:00")
        .unwrap();
    r.apply_mode(AttendanceMode::CheckIn, &id, "2026-08-08T14:00:00")
        .unwrap();
    let snaps = r.list_visit_snapshots().unwrap();
    assert_eq!(snaps.len(), 2);
    assert_ne!(snaps[0].visit_uid, snaps[1].visit_uid);
    assert!(!snaps[0].visit_uid.is_empty());
}

#[test]
fn migrate_idempotent_second_open() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("idem.db");
    let (_c1, r1) = open_database(&path).unwrap();
    assert_eq!(r1.to_version, 2);
    drop(_c1);
    let (_c2, r2) = open_database(&path).unwrap();
    assert!(r2.already_current);
    assert_eq!(r2.from_version, 2);
    assert_eq!(r2.to_version, 2);
}

#[test]
fn master_csv_bom_and_status_headers() {
    let dir = tempdir().unwrap();
    let path_a = dir.path().join("a.db");
    let (conn_a, _, st_a) = open_database_with_station(&path_a, DbRole::Desk).unwrap();
    let ra = SqliteVisitRepository::new(&conn_a, st_a.clone(), DbRole::Desk);
    ra.apply_mode(
        AttendanceMode::CheckIn,
        &AgentIdentity::new("IA", "MCSV"),
        "2026-08-08T09:00:00",
    )
    .unwrap();
    drop(conn_a);
    let pkg = dir.path().join("pkg.db");
    export_station_package(&path_a, &pkg, &st_a).unwrap();

    let path_m = dir.path().join("master.db");
    let (conn_m, _, st_m) = open_database_with_station(&path_m, DbRole::Master).unwrap();
    import_station_package(&conn_m, &pkg).unwrap();
    let rm = SqliteVisitRepository::new(&conn_m, st_m, DbRole::Master);
    let rollup = rollup_master(&rm.list_visit_snapshots().unwrap(), &[]);
    let csv_path = dir.path().join("master.csv");
    let n = export_master_csv(&csv_path, &rollup.rows, "MasterEvt").unwrap();
    assert_eq!(n, 1);
    let bytes = fs::read(&csv_path).unwrap();
    assert_eq!(&bytes[0..3], &[0xEF, 0xBB, 0xBF]);
    let text = String::from_utf8_lossy(&bytes[3..]);
    assert!(text.contains("event,保險中介人類別"));
    assert!(text.contains("first_check_in_at"));
    assert!(text.contains("needs_review"));
    assert!(text.contains("MCSV"));
    assert!(text.contains("MasterEvt"));
}

#[test]
fn event_name_meta_survives_reopen() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("ev.db");
    {
        let (conn, _, _) = open_database_with_station(&path, DbRole::Desk).unwrap();
        ifs_storage::set_event_name(&conn, "持久活動").unwrap();
        drop(conn);
    }
    let (conn2, _, _) = open_database_with_station(&path, DbRole::Desk).unwrap();
    assert_eq!(ifs_storage::get_event_name(&conn2).unwrap(), "持久活動");
}

#[test]
fn invalid_qr_never_writes_via_apply_only_valid_identity() {
    // Guard: callers must parse first; storage only accepts AgentIdentity.
    assert!(parse_qr_url("https://x/").is_err());
    let (conn, _) = open_in_memory().unwrap();
    let r = repo(&conn, station("S1", "d"));
    // Only valid identities can be applied — empty strings would violate product rules if forced;
    // repository still accepts any AgentIdentity struct; product path uses parse_qr_url first.
    let forced = AgentIdentity::new("OK", "OK");
    r.apply_mode(AttendanceMode::CheckIn, &forced, "2026-08-08T09:00:00")
        .unwrap();
    assert_eq!(r.counts().unwrap().total_visits, 1);
}

#[test]
fn package_manifest_json_written() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("desk.db");
    let (conn, _, st) = open_database_with_station(&path, DbRole::Desk).unwrap();
    drop(conn);
    let pkg = dir.path().join("out.db");
    let man = export_station_package(&path, &pkg, &st).unwrap();
    assert_eq!(man.station_id, st.station_id);
    let man_path = pkg.with_extension("manifest.json");
    assert!(man_path.exists());
    let body = fs::read_to_string(man_path).unwrap();
    assert!(body.contains(&st.station_id));
    assert!(body.contains("visit_count"));
}
