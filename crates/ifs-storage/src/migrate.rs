//! Schema migration via `PRAGMA user_version` only.

use crate::repository::StorageError;
use crate::timeutil::now_compact_local;
use rusqlite::{params, Connection};

/// Current schema version (station provenance + scan_events).
pub const SCHEMA_VERSION: i32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrateReport {
    pub from_version: i32,
    pub to_version: i32,
    pub legacy_rows_copied: u64,
    pub legacy_rows_skipped: u64,
    pub backup_table_name: Option<String>,
    pub already_current: bool,
}

pub fn migrate(conn: &mut Connection) -> Result<MigrateReport, StorageError> {
    let from = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if from > SCHEMA_VERSION {
        return Err(StorageError::Migration(format!(
            "database newer than this binary (user_version={from})"
        )));
    }
    if from < 0 {
        return Err(StorageError::Migration(format!(
            "invalid user_version={from}"
        )));
    }

    let tx = conn.transaction()?;
    let mut report = MigrateReport {
        from_version: from,
        to_version: SCHEMA_VERSION,
        legacy_rows_copied: 0,
        legacy_rows_skipped: 0,
        backup_table_name: None,
        already_current: false,
    };

    if from == SCHEMA_VERSION {
        verify_v2_schema(&tx)?;
        report.already_current = true;
        tx.commit()?;
        return Ok(report);
    }

    // Incomplete: visits exists but version 0
    if from == 0 && table_exists(&tx, "visits") {
        if !v2_indexes_complete(&tx) && !v1_indexes_complete(&tx) {
            return Err(StorageError::Migration(
                "incomplete schema (visits present but user_version=0); restore file backup".into(),
            ));
        }
    }

    if from == 0 {
        migrate_v0_to_v1(&tx, &mut report)?;
    }

    // After v0→v1, or starting from v1
    let mid: i32 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
    // We stamp versions only at end; track with report
    if from <= 1 {
        // Ensure v1 base exists
        if !table_exists(&tx, "visits") {
            create_v1_visits(&tx)?;
        }
        migrate_v1_to_v2(&tx)?;
    }

    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    verify_v2_schema(&tx)?;
    tx.commit()?;
    let _ = mid;
    Ok(report)
}

fn migrate_v0_to_v1(
    tx: &rusqlite::Transaction<'_>,
    report: &mut MigrateReport,
) -> Result<(), StorageError> {
    if table_exists(tx, "attendance") {
        // Pre-validation: duplicates
        let mut stmt = tx.prepare(
            "SELECT COALESCE(保險中介人類別, ''), 保險中介人編號, COUNT(*)
             FROM attendance
             WHERE TRIM(COALESCE(保險中介人編號, '')) != ''
             GROUP BY 1, 2
             HAVING COUNT(*) > 1",
        )?;
        let dups: Vec<(String, String, i64)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<_, _>>()?;
        if !dups.is_empty() {
            return Err(StorageError::Migration(format!(
                "legacy attendance has duplicate identity pairs: {dups:?}"
            )));
        }

        let skipped: u64 = tx.query_row(
            "SELECT COUNT(*) FROM attendance WHERE TRIM(COALESCE(保險中介人編號, '')) = ''",
            [],
            |r| r.get(0),
        )?;
        report.legacy_rows_skipped = skipped;
    }

    create_v1_visits(tx)?;

    if table_exists(tx, "attendance") {
        let copied = tx.execute(
            "INSERT INTO visits (category, license_no, check_in_at, check_out_at, created_at,
                                station_id, visit_uid, source_kind)
             SELECT
                COALESCE(保險中介人類別, ''),
                TRIM(保險中介人編號),
                CASE
                  WHEN timestamp IS NULL OR TRIM(timestamp) = '' THEN strftime('%Y-%m-%dT%H:%M:%S', 'now', 'localtime')
                  ELSE replace(trim(timestamp), ' ', 'T')
                END,
                NULL,
                CASE
                  WHEN timestamp IS NULL OR TRIM(timestamp) = '' THEN strftime('%Y-%m-%dT%H:%M:%S', 'now', 'localtime')
                  ELSE replace(trim(timestamp), ' ', 'T')
                END,
                '',
                lower(hex(randomblob(16))),
                'local'
             FROM attendance
             WHERE TRIM(COALESCE(保險中介人編號, '')) != ''",
            [],
        )?;
        report.legacy_rows_copied = copied as u64;

        let legacy_name = format!("attendance_legacy_{}", now_compact_local());
        tx.execute(
            &format!("ALTER TABLE attendance RENAME TO [{legacy_name}]"),
            [],
        )?;
        report.backup_table_name = Some(legacy_name);
    }

    // Create v1 indexes if not yet
    tx.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_visits_one_open
         ON visits (category, license_no)
         WHERE check_out_at IS NULL",
        [],
    )?;
    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_visits_identity
         ON visits (category, license_no)",
        [],
    )?;
    Ok(())
}

fn create_v1_visits(tx: &rusqlite::Transaction<'_>) -> Result<(), StorageError> {
    tx.execute(
        "CREATE TABLE IF NOT EXISTS visits (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            category      TEXT    NOT NULL,
            license_no    TEXT    NOT NULL,
            check_in_at   TEXT    NOT NULL,
            check_out_at  TEXT,
            created_at    TEXT    NOT NULL,
            notes         TEXT,
            station_id    TEXT    NOT NULL DEFAULT '',
            visit_uid     TEXT    NOT NULL DEFAULT '',
            source_kind   TEXT    NOT NULL DEFAULT 'local'
        )",
        [],
    )?;
    Ok(())
}

fn migrate_v1_to_v2(tx: &rusqlite::Transaction<'_>) -> Result<(), StorageError> {
    // Add columns if upgrading from a pure v1 without them
    ensure_column(tx, "visits", "station_id", "TEXT NOT NULL DEFAULT ''")?;
    ensure_column(tx, "visits", "visit_uid", "TEXT NOT NULL DEFAULT ''")?;
    ensure_column(tx, "visits", "source_kind", "TEXT NOT NULL DEFAULT 'local'")?;
    ensure_column(tx, "visits", "notes", "TEXT")?;

    // Backfill empty visit_uid
    tx.execute(
        "UPDATE visits SET visit_uid = lower(hex(randomblob(16)))
         WHERE visit_uid IS NULL OR visit_uid = ''",
        [],
    )?;

    tx.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_visits_uid ON visits (visit_uid)",
        [],
    )?;
    tx.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_visits_one_open
         ON visits (category, license_no)
         WHERE check_out_at IS NULL",
        [],
    )?;
    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_visits_identity
         ON visits (category, license_no)",
        [],
    )?;

    tx.execute(
        "CREATE TABLE IF NOT EXISTS scan_events (
            event_uid   TEXT PRIMARY KEY,
            station_id  TEXT NOT NULL,
            category    TEXT NOT NULL,
            license_no  TEXT NOT NULL,
            mode        TEXT NOT NULL,
            recorded_at TEXT NOT NULL,
            outcome     TEXT NOT NULL,
            visit_uid   TEXT
        )",
        [],
    )?;

    tx.execute(
        "CREATE TABLE IF NOT EXISTS app_meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
        [],
    )?;

    tx.execute(
        "CREATE TABLE IF NOT EXISTS import_audit (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            station_id TEXT NOT NULL,
            imported_at TEXT NOT NULL,
            rows_inserted INTEGER NOT NULL,
            rows_skipped INTEGER NOT NULL
        )",
        [],
    )?;

    Ok(())
}

fn ensure_column(
    tx: &rusqlite::Transaction<'_>,
    table: &str,
    col: &str,
    decl: &str,
) -> Result<(), StorageError> {
    let mut stmt = tx.prepare(&format!("PRAGMA table_info({table})"))?;
    let cols: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<Result<_, _>>()?;
    if !cols.iter().any(|c| c == col) {
        tx.execute(&format!("ALTER TABLE {table} ADD COLUMN {col} {decl}"), [])?;
    }
    Ok(())
}

fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
        params![name],
        |_| Ok(()),
    )
    .is_ok()
}

fn index_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type='index' AND name=?1",
        params![name],
        |_| Ok(()),
    )
    .is_ok()
}

fn v1_indexes_complete(conn: &Connection) -> bool {
    // Desk open-visit uniqueness index is optional on master DBs (may be dropped).
    index_exists(conn, "idx_visits_identity")
}

fn v2_indexes_complete(conn: &Connection) -> bool {
    // Required for all roles: identity lookup + visit_uid uniqueness + events table.
    // idx_visits_one_open is desk-only and intentionally absent on master after import/open.
    index_exists(conn, "idx_visits_identity")
        && index_exists(conn, "idx_visits_uid")
        && table_exists(conn, "scan_events")
}

fn verify_v2_schema(conn: &Connection) -> Result<(), StorageError> {
    if !table_exists(conn, "visits") {
        return Err(StorageError::Migration("visits table missing".into()));
    }
    if !table_exists(conn, "scan_events") {
        return Err(StorageError::Migration("scan_events missing".into()));
    }
    if !table_exists(conn, "app_meta") {
        return Err(StorageError::Migration("app_meta missing".into()));
    }
    if !v2_indexes_complete(conn) {
        return Err(StorageError::Migration("v2 indexes incomplete".into()));
    }
    Ok(())
}

/// Ensure desk partial unique index exists when opening as a desk (not master).
pub fn ensure_desk_open_unique_index(conn: &Connection) -> Result<(), StorageError> {
    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_visits_one_open
         ON visits (category, license_no)
         WHERE check_out_at IS NULL",
        [],
    )?;
    Ok(())
}

/// Drop desk-only open uniqueness so master can hold multi-station open visits.
pub fn drop_desk_open_unique_index(conn: &Connection) -> Result<(), StorageError> {
    conn.execute("DROP INDEX IF EXISTS idx_visits_one_open", [])?;
    // Non-unique identity index remains for lookups.
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_visits_identity ON visits (category, license_no)",
        [],
    )?;
    Ok(())
}
