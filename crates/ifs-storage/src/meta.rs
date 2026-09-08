//! `app_meta` key/value helpers (event name, preferences).

use crate::repository::StorageError;
use rusqlite::{params, Connection};

pub const META_EVENT_NAME: &str = "event_name";
pub const META_SOUND_ENABLED: &str = "sound_enabled";
pub const META_CPD_CHECK_IN_FROM: &str = "cpd_check_in_from";
pub const META_CPD_CHECK_IN_UNTIL: &str = "cpd_check_in_until";
pub const META_CPD_CHECK_OUT_FROM: &str = "cpd_check_out_from";
pub const META_CPD_CHECK_OUT_UNTIL: &str = "cpd_check_out_until";
pub const META_CPD_POINTS: &str = "cpd_points";

/// Raw CPD settings as edited by the operator (blank = unset).
/// Parsed/validated by [`ifs_core::CpdPolicy::from_config`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CpdConfig {
    pub check_in_from: String,
    pub check_in_until: String,
    pub check_out_from: String,
    pub check_out_until: String,
    pub points: String,
}


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

/// CPD window settings; missing keys read as blank.
pub fn get_cpd_config(conn: &Connection) -> Result<CpdConfig, StorageError> {
    Ok(CpdConfig {
        check_in_from: get_meta(conn, META_CPD_CHECK_IN_FROM)?.unwrap_or_default(),
        check_in_until: get_meta(conn, META_CPD_CHECK_IN_UNTIL)?.unwrap_or_default(),
        check_out_from: get_meta(conn, META_CPD_CHECK_OUT_FROM)?.unwrap_or_default(),
        check_out_until: get_meta(conn, META_CPD_CHECK_OUT_UNTIL)?.unwrap_or_default(),
        points: get_meta(conn, META_CPD_POINTS)?.unwrap_or_default(),
    })
}

/// Persist CPD window settings (values trimmed; blank clears a key's effect).
pub fn set_cpd_config(conn: &Connection, cfg: &CpdConfig) -> Result<(), StorageError> {
    set_meta(conn, META_CPD_CHECK_IN_FROM, cfg.check_in_from.trim())?;
    set_meta(conn, META_CPD_CHECK_IN_UNTIL, cfg.check_in_until.trim())?;
    set_meta(conn, META_CPD_CHECK_OUT_FROM, cfg.check_out_from.trim())?;
    set_meta(conn, META_CPD_CHECK_OUT_UNTIL, cfg.check_out_until.trim())?;
    set_meta(conn, META_CPD_POINTS, cfg.points.trim())?;
    Ok(())
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

    #[test]
    fn cpd_config_round_trip_and_blank_default() {
        let (conn, _) = open_in_memory().unwrap();
        assert_eq!(get_cpd_config(&conn).unwrap(), CpdConfig::default());
        let cfg = CpdConfig {
            check_in_from: " 14:30 ".into(),
            check_in_until: "15:00".into(),
            check_out_from: "17:10".into(),
            check_out_until: "17:30".into(),
            points: "2".into(),
        };
        set_cpd_config(&conn, &cfg).unwrap();
        let read = get_cpd_config(&conn).unwrap();
        assert_eq!(read.check_in_from, "14:30"); // trimmed on write
        assert_eq!(read.check_out_until, "17:30");
        assert_eq!(read.points, "2");
    }
}
