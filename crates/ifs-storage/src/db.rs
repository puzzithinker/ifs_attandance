//! Open database with PRAGMAs and migrate.

use crate::migrate::{
    drop_desk_open_unique_index, ensure_desk_open_unique_index, migrate, MigrateReport,
};
use crate::repository::{DbRole, StorageError};
use crate::station::{load_or_create_station, StationInfo};
use rusqlite::Connection;
use std::path::Path;

/// Open file DB, set busy_timeout, migrate to current schema.
pub fn open_database(path: &Path) -> Result<(Connection, MigrateReport), StorageError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut conn = Connection::open(path)?;
    conn.busy_timeout(std::time::Duration::from_millis(5000))?;
    conn.pragma_update(None, "foreign_keys", true)?;
    let report = migrate(&mut conn)?;
    Ok((conn, report))
}

/// Open DB and resolve station.toml next to it.
pub fn open_database_with_station(
    path: &Path,
    role: DbRole,
) -> Result<(Connection, MigrateReport, StationInfo), StorageError> {
    let (conn, report) = open_database(path)?;
    let (station, _) = load_or_create_station(path)?;
    // Stamp meta
    let _ = conn.execute(
        "INSERT INTO app_meta(key, value) VALUES('db_role', ?1)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        rusqlite::params![match role {
            DbRole::Desk => "desk",
            DbRole::Master => "master",
        }],
    );
    let _ = conn.execute(
        "INSERT INTO app_meta(key, value) VALUES('station_id', ?1)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        rusqlite::params![station.station_id],
    );
    // Role-specific open-visit uniqueness:
    // - Desk: enforce one open visit per identity
    // - Master: drop partial unique so multi-station opens are allowed; reopen must still migrate OK
    match role {
        DbRole::Master => drop_desk_open_unique_index(&conn)?,
        DbRole::Desk => ensure_desk_open_unique_index(&conn)?,
    }
    Ok((conn, report, station))
}

/// In-memory DB for tests (fresh schema).
pub fn open_in_memory() -> Result<(Connection, MigrateReport), StorageError> {
    let mut conn = Connection::open_in_memory()?;
    conn.busy_timeout(std::time::Duration::from_millis(5000))?;
    let report = migrate(&mut conn)?;
    Ok((conn, report))
}


