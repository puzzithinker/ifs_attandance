//! Visit repository: apply_mode, counts, CSV.

use crate::export::{default_export_filename, write_visits_csv};
use crate::station::StationInfo;
use ifs_core::{
    decide_scan, AgentIdentity, AttendanceCounts, AttendanceMode, CheckoutEvent, PersistCommand,
    ScanOutcome, VisitPresence, VisitSnapshot,
};
use rusqlite::{params, Connection};
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("migration: {0}")]
    Migration(String),
    #[error("config: {0}")]
    Config(String),
    #[error("package: {0}")]
    Package(String),
    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbRole {
    Desk,
    Master,
}

pub struct SqliteVisitRepository<'a> {
    conn: &'a Connection,
    station: StationInfo,
    role: DbRole,
    soft_checkout: bool,
}

impl<'a> SqliteVisitRepository<'a> {
    pub fn new(conn: &'a Connection, station: StationInfo, role: DbRole) -> Self {
        Self {
            conn,
            station,
            role,
            soft_checkout: true, // K24 default on
        }
    }

    pub fn with_soft_checkout(mut self, enabled: bool) -> Self {
        self.soft_checkout = enabled;
        self
    }

    pub fn station(&self) -> &StationInfo {
        &self.station
    }

    pub fn role(&self) -> DbRole {
        self.role
    }

    pub fn soft_checkout(&self) -> bool {
        self.soft_checkout
    }

    /// Load presence, call decide_scan, apply PersistCommand in one transaction.
    pub fn apply_mode(
        &self,
        mode: AttendanceMode,
        identity: &AgentIdentity,
        now_iso: &str,
    ) -> Result<ScanOutcome, StorageError> {
        let tx = self.conn.unchecked_transaction()?;

        let open: Option<(i64, String, String)> = tx
            .query_row(
                "SELECT id, check_in_at, visit_uid FROM visits
                 WHERE category = ?1 AND license_no = ?2 AND check_out_at IS NULL",
                params![identity.category, identity.license_no],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;

        let (presence, open_visit_id, open_check_in_at, open_visit_uid) = match &open {
            Some((id, at, uid)) => (
                VisitPresence::CheckedIn,
                Some(*id),
                Some(at.as_str()),
                Some(uid.as_str()),
            ),
            None => (VisitPresence::CheckedOut, None, None, None),
        };

        let (outcome, cmd) = decide_scan(
            mode,
            presence,
            identity.clone(),
            now_iso,
            open_visit_id,
            open_check_in_at,
            self.soft_checkout,
        );

        if let Some(cmd) = cmd {
            match apply_command(&tx, &cmd, &self.station, now_iso, open_visit_uid, &outcome) {
                Ok(()) => {}
                Err(StorageError::Sqlite(e)) if is_unique_violation(&e) => {
                    // Re-read open visit
                    let again: Option<(i64, String)> = tx
                        .query_row(
                            "SELECT id, check_in_at FROM visits
                             WHERE category = ?1 AND license_no = ?2 AND check_out_at IS NULL",
                            params![identity.category, identity.license_no],
                            |r| Ok((r.get(0)?, r.get(1)?)),
                        )
                        .optional()?;
                    if let Some((_id, at)) = again {
                        tx.commit()?;
                        return Ok(ScanOutcome::AlreadyCheckedIn {
                            identity: identity.clone(),
                            check_in_at: at,
                        });
                    }
                    return Err(StorageError::Other(format!("unique constraint: {e}")));
                }
                Err(e) => return Err(e),
            }
        } else {
            // Still record scan event for audit on no-op outcomes? Only for real decisions.
            record_event(
                &tx,
                &self.station.station_id,
                identity,
                mode,
                now_iso,
                outcome_label(&outcome),
                None,
            )?;
        }

        tx.commit()?;
        Ok(outcome)
    }

    pub fn counts(&self) -> Result<AttendanceCounts, StorageError> {
        let currently_inside: u64 = self.conn.query_row(
            "SELECT COUNT(*) FROM visits WHERE check_out_at IS NULL",
            [],
            |r| r.get(0),
        )?;
        let total_visits: u64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM visits", [], |r| r.get(0))?;
        let unique_agents: u64 = self.conn.query_row(
            "SELECT COUNT(*) FROM (SELECT DISTINCT category, license_no FROM visits)",
            [],
            |r| r.get(0),
        )?;
        Ok(AttendanceCounts {
            currently_inside,
            unique_agents,
            total_visits,
        })
    }

    pub fn export_csv(&self, path: &Path, event_name: &str) -> Result<u64, StorageError> {
        let visits = self.list_visit_snapshots()?;
        write_visits_csv(path, &visits, event_name)
    }

    pub fn list_visit_snapshots(&self) -> Result<Vec<VisitSnapshot>, StorageError> {
        let mut stmt = self.conn.prepare(
            "SELECT category, license_no, check_in_at, check_out_at, station_id, visit_uid
             FROM visits ORDER BY id",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(VisitSnapshot {
                    identity: AgentIdentity::new(r.get::<_, String>(0)?, r.get::<_, String>(1)?),
                    check_in_at: r.get(2)?,
                    check_out_at: r.get(3)?,
                    station_id: r.get(4)?,
                    visit_uid: r.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn list_orphan_checkouts(&self) -> Result<Vec<CheckoutEvent>, StorageError> {
        let mut stmt = self.conn.prepare(
            "SELECT event_uid, station_id, category, license_no, recorded_at
             FROM scan_events
             WHERE outcome = 'OrphanCheckOut'
             ORDER BY recorded_at",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(CheckoutEvent {
                    event_uid: r.get(0)?,
                    station_id: r.get(1)?,
                    identity: AgentIdentity::new(r.get::<_, String>(2)?, r.get::<_, String>(3)?),
                    at: r.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn default_export_name(&self, event_name: &str) -> String {
        default_export_filename(chrono::Local::now(), event_name)
    }
}

fn apply_command(
    tx: &rusqlite::Transaction<'_>,
    cmd: &PersistCommand,
    station: &StationInfo,
    now_iso: &str,
    open_visit_uid: Option<&str>,
    outcome: &ScanOutcome,
) -> Result<(), StorageError> {
    match cmd {
        PersistCommand::InsertCheckIn { identity, at } => {
            let visit_uid = Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO visits
                 (category, license_no, check_in_at, check_out_at, created_at, station_id, visit_uid, source_kind)
                 VALUES (?1, ?2, ?3, NULL, ?3, ?4, ?5, 'local')",
                params![
                    identity.category,
                    identity.license_no,
                    at,
                    station.station_id,
                    visit_uid
                ],
            )?;
            record_event(
                tx,
                &station.station_id,
                identity,
                AttendanceMode::CheckIn,
                now_iso,
                outcome_label(outcome),
                Some(&visit_uid),
            )?;
        }
        PersistCommand::UpdateCheckOut { visit_id, at } => {
            tx.execute(
                "UPDATE visits SET check_out_at = ?1 WHERE id = ?2 AND check_out_at IS NULL",
                params![at, visit_id],
            )?;
            let uid = open_visit_uid.map(|s| s.to_string());
            // Need identity from outcome
            if let ScanOutcome::CheckedOut { identity, .. } = outcome {
                record_event(
                    tx,
                    &station.station_id,
                    identity,
                    AttendanceMode::CheckOut,
                    now_iso,
                    outcome_label(outcome),
                    uid.as_deref(),
                )?;
            }
        }
        PersistCommand::InsertOrphanCheckOut { identity, at } => {
            record_event(
                tx,
                &station.station_id,
                identity,
                AttendanceMode::CheckOut,
                at,
                "OrphanCheckOut",
                None,
            )?;
        }
    }
    Ok(())
}

fn record_event(
    tx: &rusqlite::Transaction<'_>,
    station_id: &str,
    identity: &AgentIdentity,
    mode: AttendanceMode,
    recorded_at: &str,
    outcome: &str,
    visit_uid: Option<&str>,
) -> Result<(), StorageError> {
    let mode_s = match mode {
        AttendanceMode::CheckIn => "check_in",
        AttendanceMode::CheckOut => "check_out",
    };
    tx.execute(
        "INSERT INTO scan_events
         (event_uid, station_id, category, license_no, mode, recorded_at, outcome, visit_uid)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            Uuid::new_v4().to_string(),
            station_id,
            identity.category,
            identity.license_no,
            mode_s,
            recorded_at,
            outcome,
            visit_uid
        ],
    )?;
    Ok(())
}

fn outcome_label(o: &ScanOutcome) -> &'static str {
    match o {
        ScanOutcome::CheckedIn { .. } => "CheckedIn",
        ScanOutcome::AlreadyCheckedIn { .. } => "AlreadyCheckedIn",
        ScanOutcome::CheckedOut { .. } => "CheckedOut",
        ScanOutcome::OrphanCheckOut { .. } => "OrphanCheckOut",
        ScanOutcome::NotCheckedIn { .. } => "NotCheckedIn",
        ScanOutcome::InvalidQr { .. } => "InvalidQr",
        ScanOutcome::EmptyInput => "EmptyInput",
        ScanOutcome::Failed { .. } => "Failed",
    }
}

fn is_unique_violation(e: &rusqlite::Error) -> bool {
    match e {
        rusqlite::Error::SqliteFailure(err, _) => {
            err.code == rusqlite::ErrorCode::ConstraintViolation
        }
        _ => false,
    }
}

/// Optional query helper.
trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
