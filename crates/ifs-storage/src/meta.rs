//! `app_meta` key/value helpers (event name, preferences).

use crate::repository::StorageError;
use rusqlite::{params, Connection};

pub const META_EVENT_NAME: &str = "event_name";
pub const META_SOUND_ENABLED: &str = "sound_enabled";

pub fn get_meta(conn: &Connection, key: &str) -> Result<Option<String>, StorageError> {
    let mut stmt = conn.prepare("SELECT value FROM app_meta WHERE key = ?1")?;
    let mut rows = stmt.query(params![key])?;
    if let Some(row) = rows.next()? {
        Ok(Some(row.get(0)?))
    } else {
        Ok(None)
    }
}

pub fn set_meta(conn: &Connection, key: &str, value: &str) -> Result<(), StorageError> {
    conn.execute(
        "INSERT INTO app_meta(key, value) VALUES(?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn get_event_name(conn: &Connection) -> Result<String, StorageError> {
    Ok(get_meta(conn, META_EVENT_NAME)?.unwrap_or_default())
}

pub fn set_event_name(conn: &Connection, name: &str) -> Result<(), StorageError> {
    set_meta(conn, META_EVENT_NAME, name.trim())
}

pub fn get_sound_enabled(conn: &Connection) -> Result<bool, StorageError> {
    match get_meta(conn, META_SOUND_ENABLED)? {
        Some(v) => Ok(v != "0" && !v.eq_ignore_ascii_case("false")),
        None => Ok(true), // default on for kiosk queues
    }
}

pub fn set_sound_enabled(conn: &Connection, enabled: bool) -> Result<(), StorageError> {
    set_meta(conn, META_SOUND_ENABLED, if enabled { "1" } else { "0" })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open_in_memory;

    #[test]
    fn event_name_round_trip() {
        let (conn, _) = open_in_memory().unwrap();
        assert_eq!(get_event_name(&conn).unwrap(), "");
        set_event_name(&conn, "  CPD 下午場  ").unwrap();
        assert_eq!(get_event_name(&conn).unwrap(), "CPD 下午場");
        set_event_name(&conn, "另一場").unwrap();
        assert_eq!(get_event_name(&conn).unwrap(), "另一場");
    }

    #[test]
    fn sound_default_true_then_persist() {
        let (conn, _) = open_in_memory().unwrap();
        assert!(get_sound_enabled(&conn).unwrap());
        set_sound_enabled(&conn, false).unwrap();
        assert!(!get_sound_enabled(&conn).unwrap());
        set_sound_enabled(&conn, true).unwrap();
        assert!(get_sound_enabled(&conn).unwrap());
    }
}
