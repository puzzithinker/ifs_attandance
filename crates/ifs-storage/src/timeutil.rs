use chrono::Local;

/// Local wall clock ISO without timezone: `YYYY-MM-DDTHH:MM:SS`.
pub fn now_iso_local() -> String {
    Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// Compact timestamp for table/file names: `YYYYMMDDHHMMSS`.
pub fn now_compact_local() -> String {
    Local::now().format("%Y%m%d%H%M%S").to_string()
}
