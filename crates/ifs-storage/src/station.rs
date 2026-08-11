//! Per-install station identity.

use crate::repository::StorageError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StationInfo {
    pub station_id: String,
    pub station_name: String,
}

impl StationInfo {
    pub fn new(station_id: impl Into<String>, station_name: impl Into<String>) -> Self {
        Self {
            station_id: station_id.into(),
            station_name: station_name.into(),
        }
    }
}

/// Load `station.toml` next to the DB, or create with a new UUID.
pub fn load_or_create_station(db_path: &Path) -> Result<(StationInfo, PathBuf), StorageError> {
    let path = station_path_for_db(db_path);
    if path.exists() {
        let text = fs::read_to_string(&path)?;
        let info = parse_station_toml(&text)?;
        return Ok((info, path));
    }
    let info = StationInfo::new(Uuid::new_v4().to_string(), "desk-1");
    write_station_file(&path, &info)?;
    Ok((info, path))
}

pub fn station_path_for_db(db_path: &Path) -> PathBuf {
    let dir = db_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    dir.join("station.toml")
}

pub fn write_station_file(path: &Path, info: &StationInfo) -> Result<(), StorageError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = format!(
        "station_id = \"{}\"\nstation_name = \"{}\"\n",
        escape_toml(&info.station_id),
        escape_toml(&info.station_name)
    );
    fs::write(path, body)?;
    Ok(())
}

fn escape_toml(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn parse_station_toml(text: &str) -> Result<StationInfo, StorageError> {
    let mut station_id = None;
    let mut station_name = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            let v = v.trim().trim_matches('"');
            match k {
                "station_id" => station_id = Some(v.to_string()),
                "station_name" => station_name = Some(v.to_string()),
                _ => {}
            }
        }
    }
    let station_id =
        station_id.ok_or_else(|| StorageError::Config("station.toml missing station_id".into()))?;
    let station_name = station_name.unwrap_or_else(|| "desk".into());
    if station_id.is_empty() {
        return Err(StorageError::Config("station_id empty".into()));
    }
    Ok(StationInfo::new(station_id, station_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn create_and_reload() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("agent.db");
        let (a, path) = load_or_create_station(&db).unwrap();
        assert!(path.exists());
        let (b, _) = load_or_create_station(&db).unwrap();
        assert_eq!(a.station_id, b.station_id);
    }

    #[test]
    fn rename_station_persists() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("agent.db");
        let (mut info, path) = load_or_create_station(&db).unwrap();
        info.station_name = "出口-2".into();
        write_station_file(&path, &info).unwrap();
        let (again, _) = load_or_create_station(&db).unwrap();
        assert_eq!(again.station_name, "出口-2");
        assert_eq!(again.station_id, info.station_id);
    }
}
