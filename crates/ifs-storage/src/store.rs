//! `AttendanceStore`: owns the SQLite connection and exposes every storage
//! operation the GUI needs, including unified scan orchestration
//! (raw scan string → `ScanOutcome`).

use crate::export::export_master_csv;
use crate::meta::{
    get_cpd_config, get_event_name, get_sound_enabled, set_cpd_config, set_event_name,
    set_sound_enabled, CpdConfig,
};
use crate::migrate::{drop_desk_open_unique_index, ensure_desk_open_unique_index};
use crate::package::{
    export_station_package, import_station_package, ImportReport, PackageManifest,
};
use crate::repository::{DbRole, SqliteVisitRepository, StorageError};
use crate::station::{station_path_for_db, write_station_file, StationInfo};
use ifs_core::{
    outcome_from_parse_error, parse_qr_url, rollup_master, AgentIdentity, AttendanceCounts,
    AttendanceMode, MasterAgentRow, ScanOutcome,
};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Result of a scan orchestration: the outcome, the parsed identity (if any),
/// and the timestamp supplied by the caller (echoed back for UI display).
#[derive(Debug, Clone, PartialEq)]
pub struct ScanResult {
    pub outcome: ScanOutcome,
    pub identity: Option<AgentIdentity>,
    pub at: String,
}

/// Owns the SQLite connection and station metadata; exposes all storage
/// operations the GUI needs without leaking `rusqlite` types.
pub struct AttendanceStore {
    conn: Connection,
    station: StationInfo,
    role: DbRole,
    soft_checkout: bool,
    db_path: PathBuf,
    station_path: PathBuf,
}

impl AttendanceStore {
    /// Open a file-backed database, migrate it, and load/create station.toml.
    pub fn open(db_path: &Path, role: DbRole, soft_checkout: bool) -> Result<Self, StorageError> {
        let (conn, _report, station) = crate::db::open_database_with_station(db_path, role)?;
        let station_path = station_path_for_db(db_path);
        Ok(Self {
            conn,
            station,
            role,
            soft_checkout,
            db_path: db_path.to_path_buf(),
            station_path,
        })
    }

    /// Open an in-memory database for tests. Station.toml persistence is
    /// disabled (station_path is empty); `rename_station` updates in-memory
    /// state only. Soft checkout defaults to true.
    pub fn open_in_memory(role: DbRole) -> Result<Self, StorageError> {
        let (conn, _report) = crate::db::open_in_memory()?;
        let station = StationInfo::new(Uuid::new_v4().to_string(), "desk-1");
        // Stamp meta + role-specific index, mirroring open_database_with_station.
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
        match role {
            DbRole::Master => drop_desk_open_unique_index(&conn)?,
            DbRole::Desk => ensure_desk_open_unique_index(&conn)?,
        }
        Ok(Self {
            conn,
            station,
            role,
            soft_checkout: true,
            db_path: PathBuf::new(),
            station_path: PathBuf::new(),
        })
    }

    /// Build a short-lived repository borrowing the owned connection.
    fn repo(&self) -> SqliteVisitRepository<'_> {
        SqliteVisitRepository::new(&self.conn, self.station.clone(), self.role)
            .with_soft_checkout(self.soft_checkout)
    }

    /// Parse a raw scan string, apply the mode, and return a [`ScanResult`].
    ///
    /// Never returns `Err`: parse failures become `InvalidQr`/`EmptyInput`,
    /// DB failures become `ScanOutcome::Failed`. The `at` timestamp is
    /// supplied by the caller (clock injection) and echoed back unchanged.
    pub fn handle_scan(&self, raw: &str, mode: AttendanceMode, at: &str) -> ScanResult {
        match parse_qr_url(raw) {
            Ok(identity) => {
                let outcome = match self.repo().apply_mode(mode, &identity, at) {
                    Ok(o) => o,
                    Err(e) => ScanOutcome::Failed {
                        message: e.to_string(),
                    },
                };
                ScanResult {
                    outcome,
                    identity: Some(identity),
                    at: at.to_string(),
                }
            }
            Err(e) => ScanResult {
                outcome: outcome_from_parse_error(&e),
                identity: None,
                at: at.to_string(),
            },
        }
    }

    /// Live counts for the owned database.
    pub fn counts(&self) -> Result<AttendanceCounts, StorageError> {
        self.repo().counts()
    }

    /// Master rollup rows: snapshots + orphans + `rollup_master` internally.
    pub fn master_rollup_rows(&self) -> Result<Vec<MasterAgentRow>, StorageError> {
        let repo = self.repo();
        let visits = repo.list_visit_snapshots()?;
        let orphans = repo.list_orphan_checkouts()?;
        Ok(rollup_master(&visits, &orphans).rows)
    }

    /// Write desk/detail visits CSV; returns row count (excluding header).
    pub fn export_csv(&self, dest: &Path, event_name: &str) -> Result<u64, StorageError> {
        self.repo().export_csv(dest, event_name)
    }

    /// Raw CPD window settings from `app_meta` (blank = unset).
    pub fn cpd_config(&self) -> Result<CpdConfig, StorageError> {
        get_cpd_config(&self.conn)
    }

    /// Persist CPD window settings to `app_meta`.
    pub fn set_cpd_config(&self, cfg: &CpdConfig) -> Result<(), StorageError> {
        set_cpd_config(&self.conn, cfg)
    }

    /// Write master rollup CSV; returns row count (excluding header).
    pub fn export_master_csv(&self, dest: &Path, event_name: &str) -> Result<u64, StorageError> {
        let rows = self.master_rollup_rows()?;
        let policy = self.repo().cpd_policy()?;
        export_master_csv(dest, &rows, event_name, policy.as_ref())
    }

    /// Copy desk DB to package path and write sibling `.manifest.json`.
    pub fn export_package(&self, dest: &Path) -> Result<PackageManifest, StorageError> {
        export_station_package(&self.db_path, dest, &self.station)
    }

    /// Import visits + scan_events from a station package DB (INSERT OR IGNORE).
    pub fn import_package(&self, src: &Path) -> Result<ImportReport, StorageError> {
        import_station_package(&self.conn, src)
    }

    /// Current event name from `app_meta` (empty string if unset).
    pub fn event_name(&self) -> String {
        get_event_name(&self.conn).unwrap_or_default()
    }

    /// Persist the event name into `app_meta`.
    pub fn set_event_name(&self, name: &str) -> Result<(), StorageError> {
        set_event_name(&self.conn, name)
    }

    /// Sound preference from `app_meta` (defaults to `true` when unset).
    pub fn sound_enabled(&self) -> bool {
        get_sound_enabled(&self.conn).unwrap_or(true)
    }

    /// Persist the sound preference into `app_meta`.
    pub fn set_sound_enabled(&self, enabled: bool) -> Result<(), StorageError> {
        set_sound_enabled(&self.conn, enabled)
    }

    /// Borrow the station info.
    pub fn station(&self) -> &StationInfo {
        &self.station
    }

    /// Update the in-memory station name and persist to `station.toml`
    /// (skips file write when station_path is empty, e.g. in-memory stores).
    pub fn rename_station(&mut self, new_name: &str) -> Result<(), StorageError> {
        self.station.station_name = new_name.to_string();
        if !self.station_path.as_os_str().is_empty() {
            write_station_file(&self.station_path, &self.station)?;
        }
        Ok(())
    }

    /// Database role (Desk or Master).
    pub fn role(&self) -> DbRole {
        self.role
    }

    /// Soft check-out flag.
    pub fn soft_checkout(&self) -> bool {
        self.soft_checkout
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ifs_core::AttendanceMode;
    use tempfile::tempdir;

    fn valid_qr(lic: &str) -> String {
        format!("https://example.hk/?categoryCode=IA&licenseNo={lic}")
    }

    #[test]
    fn checkin_writes_visit() {
        let store = AttendanceStore::open_in_memory(DbRole::Desk).unwrap();
        let r = store.handle_scan(
            &valid_qr("A1"),
            AttendanceMode::CheckIn,
            "2026-08-08T09:00:00",
        );
        assert!(matches!(r.outcome, ScanOutcome::CheckedIn { .. }));
        assert!(r.identity.is_some());
        assert_eq!(r.at, "2026-08-08T09:00:00");
        assert_eq!(store.counts().unwrap().total_visits, 1);
    }

    #[test]
    fn duplicate_checkin_returns_already_checked_in_and_writes_nothing() {
        let store = AttendanceStore::open_in_memory(DbRole::Desk).unwrap();
        store.handle_scan(
            &valid_qr("DUP"),
            AttendanceMode::CheckIn,
            "2026-08-08T09:00:00",
        );
        let r = store.handle_scan(
            &valid_qr("DUP"),
            AttendanceMode::CheckIn,
            "2026-08-08T09:05:00",
        );
        assert!(matches!(r.outcome, ScanOutcome::AlreadyCheckedIn { .. }));
        assert_eq!(store.counts().unwrap().total_visits, 1);
    }

    #[test]
    fn checkout_after_checkin_returns_checked_out() {
        let store = AttendanceStore::open_in_memory(DbRole::Desk).unwrap();
        store.handle_scan(
            &valid_qr("CO"),
            AttendanceMode::CheckIn,
            "2026-08-08T09:00:00",
        );
        let r = store.handle_scan(
            &valid_qr("CO"),
            AttendanceMode::CheckOut,
            "2026-08-08T12:00:00",
        );
        assert!(matches!(r.outcome, ScanOutcome::CheckedOut { .. }));
        assert_eq!(store.counts().unwrap().currently_inside, 0);
    }

    #[test]
    fn soft_checkout_no_open_visit_returns_orphan() {
        let store = AttendanceStore::open_in_memory(DbRole::Desk).unwrap();
        let r = store.handle_scan(
            &valid_qr("ORPH"),
            AttendanceMode::CheckOut,
            "2026-08-08T12:00:00",
        );
        assert!(matches!(r.outcome, ScanOutcome::OrphanCheckOut { .. }));
    }

    #[test]
    fn garbage_qr_returns_invalid_and_writes_nothing() {
        let store = AttendanceStore::open_in_memory(DbRole::Desk).unwrap();
        let before = store.counts().unwrap().total_visits;
        let r = store.handle_scan("garbage", AttendanceMode::CheckIn, "2026-08-08T09:00:00");
        assert!(matches!(r.outcome, ScanOutcome::InvalidQr { .. }));
        assert!(r.identity.is_none());
        assert_eq!(store.counts().unwrap().total_visits, before);
    }

    #[test]
    fn empty_input_returns_empty_input() {
        let store = AttendanceStore::open_in_memory(DbRole::Desk).unwrap();
        let r = store.handle_scan("", AttendanceMode::CheckIn, "2026-08-08T09:00:00");
        assert!(matches!(r.outcome, ScanOutcome::EmptyInput));
    }

    #[test]
    fn event_name_and_sound_round_trip() {
        let store = AttendanceStore::open_in_memory(DbRole::Desk).unwrap();
        assert_eq!(store.event_name(), "");
        store.set_event_name("測試活動").unwrap();
        assert_eq!(store.event_name(), "測試活動");
        assert!(store.sound_enabled());
        store.set_sound_enabled(false).unwrap();
        assert!(!store.sound_enabled());
        store.set_sound_enabled(true).unwrap();
        assert!(store.sound_enabled());
    }

    #[test]
    fn rename_station_updates_in_memory_name() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("test.db");
        let mut store = AttendanceStore::open(&db, DbRole::Desk, true).unwrap();
        let original = store.station().station_name.clone();
        store.rename_station("出口-2").unwrap();
        assert_eq!(store.station().station_name, "出口-2");
        assert_ne!(store.station().station_name, original);
        // Verify station.toml was written
        let station_path = dir.path().join("station.toml");
        assert!(station_path.exists());
        let body = std::fs::read_to_string(&station_path).unwrap();
        assert!(body.contains("出口-2"));
    }
}
