//! SQLite storage, migration, CSV export, station packages, and master import.

mod db;
mod export;
mod meta;
mod migrate;
mod package;
mod repository;
mod station;
mod store;
mod timeutil;

pub use db::{open_database, open_database_with_station, open_in_memory};
pub use export::{
    default_export_filename, export_master_csv, master_export_filename, sanitize_event_slug,
};
pub use meta::{
    get_event_name, get_sound_enabled, set_event_name, set_sound_enabled, META_EVENT_NAME,
};
pub use migrate::{
    drop_desk_open_unique_index, ensure_desk_open_unique_index, migrate, MigrateReport,
};
pub use package::{export_station_package, import_station_package, ImportReport, PackageManifest};
pub use repository::{DbRole, SqliteVisitRepository, StorageError};
pub use station::{load_or_create_station, station_path_for_db, write_station_file, StationInfo};
pub use store::{AttendanceStore, ScanResult};
pub use timeutil::{now_compact_local, now_iso_local};
