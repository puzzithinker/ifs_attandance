//! Station package export and master import (idempotent by visit_uid).

use crate::db::open_database;
use crate::repository::StorageError;
use crate::station::StationInfo;
use crate::timeutil::now_iso_local;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageManifest {
    pub station_id: String,
    pub station_name: String,
    pub exported_at: String,
    pub app_version: String,
    pub visit_count: u64,
    pub event_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportReport {
    pub station_id: String,
    pub rows_inserted: u64,
    pub rows_skipped: u64,
    pub events_inserted: u64,
    pub events_skipped: u64,
}

/// Copy desk DB to package path and write sibling `.manifest.json`.
pub fn export_station_package(
    source_db: &Path,
    dest_db: &Path,
    station: &StationInfo,
) -> Result<PackageManifest, StorageError> {
    if let Some(parent) = dest_db.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source_db, dest_db)?;

    let conn = Connection::open(dest_db)?;
    let visit_count: u64 = conn.query_row("SELECT COUNT(*) FROM visits", [], |r| r.get(0))?;
    let event_count: u64 = conn.query_row("SELECT COUNT(*) FROM scan_events", [], |r| r.get(0))?;
    drop(conn);

    let manifest = PackageManifest {
        station_id: station.station_id.clone(),
        station_name: station.station_name.clone(),
        exported_at: now_iso_local(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        visit_count,
        event_count,
    };
    let man_path = dest_db.with_extension("manifest.json");
    fs::write(
        man_path,
        serde_json::to_string_pretty(&manifest)
            .map_err(|e| StorageError::Package(e.to_string()))?,
    )?;
    Ok(manifest)
}

/// Import visits + scan_events from a station package DB into master (INSERT OR IGNORE by uid).
pub fn import_station_package(
    master: &Connection,
    package_db: &Path,
) -> Result<ImportReport, StorageError> {
    let (pkg, _report) = open_database(package_db)?;
    // Read station from package meta or first visit
    let station_id: String = pkg
        .query_row(
            "SELECT value FROM app_meta WHERE key='station_id'",
            [],
            |r| r.get(0),
        )
        .or_else(|_| {
            pkg.query_row(
                "SELECT station_id FROM visits WHERE station_id != '' LIMIT 1",
                [],
                |r| r.get(0),
            )
        })
        .unwrap_or_else(|_| "unknown".into());

    let mut rows_inserted = 0u64;
    let mut rows_skipped = 0u64;
    let mut events_inserted = 0u64;
    let mut events_skipped = 0u64;

    let tx = master.unchecked_transaction()?;

    {
        let mut stmt = pkg.prepare(
            "SELECT category, license_no, check_in_at, check_out_at, created_at, notes,
                    station_id, visit_uid, source_kind
             FROM visits",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let category: String = r.get(0)?;
            let license_no: String = r.get(1)?;
            let check_in_at: String = r.get(2)?;
            let check_out_at: Option<String> = r.get(3)?;
            let created_at: String = r.get(4)?;
            let notes: Option<String> = r.get(5)?;
            let st: String = r.get(6)?;
            let visit_uid: String = r.get(7)?;
            let _sk: String = r.get(8)?;

            let changed = tx.execute(
                "INSERT OR IGNORE INTO visits
                 (category, license_no, check_in_at, check_out_at, created_at, notes,
                  station_id, visit_uid, source_kind)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'imported')",
                params![
                    category,
                    license_no,
                    check_in_at,
                    check_out_at,
                    created_at,
                    notes,
                    st,
                    visit_uid
                ],
            )?;
            if changed == 0 {
                rows_skipped += 1;
            } else {
                rows_inserted += 1;
            }
        }
    }

    {
        let mut stmt = pkg.prepare(
            "SELECT event_uid, station_id, category, license_no, mode, recorded_at, outcome, visit_uid
             FROM scan_events",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let event_uid: String = r.get(0)?;
            let st: String = r.get(1)?;
            let category: String = r.get(2)?;
            let license_no: String = r.get(3)?;
            let mode: String = r.get(4)?;
            let recorded_at: String = r.get(5)?;
            let outcome: String = r.get(6)?;
            let visit_uid: Option<String> = r.get(7)?;
            let changed = tx.execute(
                "INSERT OR IGNORE INTO scan_events
                 (event_uid, station_id, category, license_no, mode, recorded_at, outcome, visit_uid)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    event_uid, st, category, license_no, mode, recorded_at, outcome, visit_uid
                ],
            )?;
            if changed == 0 {
                events_skipped += 1;
            } else {
                events_inserted += 1;
            }
        }
    }

    tx.execute(
        "INSERT INTO import_audit(station_id, imported_at, rows_inserted, rows_skipped)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            station_id,
            now_iso_local(),
            rows_inserted as i64,
            rows_skipped as i64
        ],
    )?;

    // Master must not enforce global one-open across stations (import may add dual opens).
    // Schema verify must not require idx_visits_one_open after this drop.
    let _ = tx.execute("DROP INDEX IF EXISTS idx_visits_one_open", []);
    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_visits_identity ON visits (category, license_no)",
        [],
    )?;

    tx.commit()?;

    Ok(ImportReport {
        station_id,
        rows_inserted,
        rows_skipped,
        events_inserted,
        events_skipped,
    })
}
