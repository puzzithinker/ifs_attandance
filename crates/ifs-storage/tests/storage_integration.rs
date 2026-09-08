//! Integration tests driving shipped storage APIs (real rusqlite paths).

use ifs_core::{
    parse_qr_url, rollup_master, AgentIdentity, AttendanceMode, MasterStatus, ScanOutcome,
};
use ifs_storage::{
    default_export_filename, export_master_csv, export_station_package, import_station_package,
    open_database, open_database_with_station, open_in_memory, AttendanceStore, CpdConfig, DbRole,
    SqliteVisitRepository, StationInfo,
};
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};
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
fn desk_csv_cpd_column_from_configured_windows() {
    let dir = tempdir().unwrap();
    let store = AttendanceStore::open_in_memory(DbRole::Desk).unwrap();
    store
        .set_cpd_config(&CpdConfig {
            check_in_from: "14:30".into(),
            check_in_until: "15:00".into(),
            check_out_from: "17:10".into(),
            check_out_until: "17:30".into(),
            points: "2".into(),
        })
        .unwrap();

    let url = |lic: &str| format!("https://example.hk/?categoryCode=IA&licenseNo={lic}");
    // Full session: earned.
    let r = store.handle_scan(&url("OK1"), AttendanceMode::CheckIn, "2026-09-10T14:45:00");
    assert!(matches!(r.outcome, ScanOutcome::CheckedIn { .. }));
    let r = store.handle_scan(&url("OK1"), AttendanceMode::CheckOut, "2026-09-10T17:20:00");
    assert!(matches!(r.outcome, ScanOutcome::CheckedOut { .. }));
    // Left too early: missed.
    let r = store.handle_scan(&url("EARLY"), AttendanceMode::CheckIn, "2026-09-10T14:50:00");
    assert!(matches!(r.outcome, ScanOutcome::CheckedIn { .. }));
    let r = store.handle_scan(&url("EARLY"), AttendanceMode::CheckOut, "2026-09-10T16:30:00");
    assert!(matches!(r.outcome, ScanOutcome::CheckedOut { .. }));

    let path = dir.path().join("cpd.csv");
    let n = store.export_csv(&path, "CPD Event").unwrap();
    assert_eq!(n, 2);
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("入場時間,離場時間,CPD,visit_uid"));
    assert!(text.contains("IA,OK1,2026-09-10T14:45:00,2026-09-10T17:20:00,2,"));
    assert!(text.contains("IA,EARLY,2026-09-10T14:50:00,2026-09-10T16:30:00,0,"));
}

#[test]
fn malformed_cpd_config_fails_export_loudly() {
    let dir = tempdir().unwrap();
    let store = AttendanceStore::open_in_memory(DbRole::Desk).unwrap();
    store
        .set_cpd_config(&CpdConfig {
            check_in_from: "bananas".into(),
            ..CpdConfig::default()
        })
        .unwrap();
    let path = dir.path().join("bad.csv");
    let err = store.export_csv(&path, "").unwrap_err();
    assert!(err.to_string().contains("check-in from"), "got: {err}");
}

#[test]
fn soft_checkout_orphan_and_cross_station_pair() {
    let dir = tempdir().unwrap();
    // Station A: check-in only
    let path_a = dir.path().join("a.db");
    let (conn_a, _, st_a) = open_database_with_station(&path_a, DbRole::Desk).unwrap();
    let ra =
        SqliteVisitRepository::new(&conn_a, st_a.clone(), DbRole::Desk).with_soft_checkout(true);
    let id = AgentIdentity::new("IA", "MULTI1");
    ra.apply_mode(AttendanceMode::CheckIn, &id, "2026-08-08T10:00:00")
        .unwrap();
    drop(conn_a);

    // Station B: soft check-out
    let path_b = dir.path().join("b.db");
    let (conn_b, _, st_b) = open_database_with_station(&path_b, DbRole::Desk).unwrap();
    let rb =
        SqliteVisitRepository::new(&conn_b, st_b.clone(), DbRole::Desk).with_soft_checkout(true);
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
    export_master_csv(&master_csv, &rollup.rows, "CrossDoor", None).unwrap();
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
    let n = export_master_csv(&csv_path, &rollup.rows, "MasterEvt", None).unwrap();
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

// Crash/resume simulation: drop the store = program closed, reopen same file = restart.

fn url(cat: &str, lic: &str) -> String {
    format!("https://example.hk/?categoryCode={cat}&licenseNo={lic}")
}

fn reopen(db: &Path, role: DbRole, soft: bool) -> AttendanceStore {
    AttendanceStore::open(db, role, soft).unwrap()
}

#[test]
fn restart_after_check_in_keeps_open_visit() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("agent.db");
    let store = reopen(&db, DbRole::Desk, true);
    let r = store.handle_scan(
        &url("IA", "A-1"),
        AttendanceMode::CheckIn,
        "2026-08-08T09:00:00",
    );
    assert!(matches!(r.outcome, ScanOutcome::CheckedIn { .. }));
    drop(store);

    let store = reopen(&db, DbRole::Desk, true);
    let c = store.counts().unwrap();
    assert_eq!(c.currently_inside, 1);
    assert_eq!(c.total_visits, 1);

    let r = store.handle_scan(
        &url("IA", "A-1"),
        AttendanceMode::CheckIn,
        "2026-08-08T09:05:00",
    );
    assert!(matches!(r.outcome, ScanOutcome::AlreadyCheckedIn { .. }));
    assert_eq!(store.counts().unwrap().total_visits, 1);

    let r = store.handle_scan(
        &url("IA", "A-1"),
        AttendanceMode::CheckOut,
        "2026-08-08T12:00:00",
    );
    assert!(matches!(r.outcome, ScanOutcome::CheckedOut { .. }));
    assert_eq!(store.counts().unwrap().currently_inside, 0);
}

#[test]
fn restart_after_check_out_keeps_visit_closed() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("agent.db");
    let store = reopen(&db, DbRole::Desk, false);
    store.handle_scan(
        &url("IA", "A-2"),
        AttendanceMode::CheckIn,
        "2026-08-08T09:00:00",
    );
    store.handle_scan(
        &url("IA", "A-2"),
        AttendanceMode::CheckOut,
        "2026-08-08T12:00:00",
    );
    drop(store);

    let store = reopen(&db, DbRole::Desk, false);
    let c = store.counts().unwrap();
    assert_eq!(c.currently_inside, 0);
    assert_eq!(c.total_visits, 1);

    let r = store.handle_scan(
        &url("IA", "A-2"),
        AttendanceMode::CheckOut,
        "2026-08-08T13:00:00",
    );
    assert!(matches!(r.outcome, ScanOutcome::NotCheckedIn { .. }));
    assert_eq!(store.counts().unwrap().total_visits, 1);
}

#[test]
fn restart_after_orphan_checkout_keeps_audit() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("agent.db");
    let store = reopen(&db, DbRole::Desk, true);
    let r = store.handle_scan(
        &url("BR", "B-9"),
        AttendanceMode::CheckOut,
        "2026-08-08T12:00:00",
    );
    assert!(matches!(r.outcome, ScanOutcome::OrphanCheckOut { .. }));
    drop(store);

    let store = reopen(&db, DbRole::Desk, true);
    assert_eq!(store.counts().unwrap().total_visits, 0);
    let rows = store.master_rollup_rows().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].identity.license_no, "B-9");
    assert_eq!(rows[0].status, MasterStatus::OrphanOutOnly);
}

#[test]
fn restart_keeps_event_sound_and_station_identity() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("agent.db");
    let mut store = reopen(&db, DbRole::Desk, true);
    store.set_event_name("CPD 下午場").unwrap();
    store.set_sound_enabled(false).unwrap();
    store.rename_station("出口-2").unwrap();
    let station_id = store.station().station_id.clone();
    drop(store);

    let store = reopen(&db, DbRole::Desk, true);
    assert_eq!(store.event_name(), "CPD 下午場");
    assert!(!store.sound_enabled());
    assert_eq!(store.station().station_name, "出口-2");
    assert_eq!(store.station().station_id, station_id);
}

#[test]
fn master_reimport_after_restart_is_idempotent() {
    let dir = tempdir().unwrap();
    let desk_db = dir.path().join("desk.db");
    let desk = reopen(&desk_db, DbRole::Desk, true);
    desk.handle_scan(
        &url("IA", "A-1"),
        AttendanceMode::CheckIn,
        "2026-08-08T09:00:00",
    );
    desk.handle_scan(
        &url("IA", "A-2"),
        AttendanceMode::CheckIn,
        "2026-08-08T09:01:00",
    );
    let pkg = dir.path().join("pkg.db");
    desk.export_package(&pkg).unwrap();

    let master_db = dir.path().join("master.db");
    let master = reopen(&master_db, DbRole::Master, true);
    let first = master.import_package(&pkg).unwrap();
    assert_eq!(first.rows_inserted, 2);
    drop(master);

    let master = reopen(&master_db, DbRole::Master, true);
    let second = master.import_package(&pkg).unwrap();
    assert_eq!(second.rows_inserted, 0);
    assert_eq!(second.rows_skipped, 2);
    assert_eq!(master.counts().unwrap().total_visits, 2);
}

#[test]
fn recheck_in_after_restart_creates_new_visit() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("agent.db");
    let store = reopen(&db, DbRole::Desk, true);
    store.handle_scan(
        &url("IA", "A-4"),
        AttendanceMode::CheckIn,
        "2026-08-08T09:00:00",
    );
    store.handle_scan(
        &url("IA", "A-4"),
        AttendanceMode::CheckOut,
        "2026-08-08T12:00:00",
    );
    drop(store);

    let store = reopen(&db, DbRole::Desk, true);
    // Laptop rebooted over lunch; the agent returns and checks in again.
    let r = store.handle_scan(
        &url("IA", "A-4"),
        AttendanceMode::CheckIn,
        "2026-08-08T14:00:00",
    );
    assert!(matches!(r.outcome, ScanOutcome::CheckedIn { .. }));
    let c = store.counts().unwrap();
    assert_eq!(c.total_visits, 2);
    assert_eq!(c.currently_inside, 1);
}

#[test]
fn check_out_at_same_second_as_check_in_is_consistent() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("agent.db");
    let store = reopen(&db, DbRole::Desk, true);
    store.handle_scan(
        &url("IA", "A-3"),
        AttendanceMode::CheckIn,
        "2026-08-08T09:00:00",
    );
    let r = store.handle_scan(
        &url("IA", "A-3"),
        AttendanceMode::CheckOut,
        "2026-08-08T09:00:00",
    );
    assert!(matches!(r.outcome, ScanOutcome::CheckedOut { .. }));
    let c = store.counts().unwrap();
    assert_eq!(c.currently_inside, 0);
    assert_eq!(c.total_visits, 1);
}
