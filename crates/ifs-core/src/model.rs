//! Domain types for attendance identity, visits, and scan outcomes.

/// Insurance intermediary identity extracted from 一戶通 QR URL.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentIdentity {
    /// 保險中介人類別 — from query `categoryCode` (non-empty).
    pub category: String,
    /// 保險中介人編號 — from query `licenseNo` (non-empty).
    pub license_no: String,
}

impl AgentIdentity {
    pub fn new(category: impl Into<String>, license_no: impl Into<String>) -> Self {
        Self {
            category: category.into(),
            license_no: license_no.into(),
        }
    }
}

/// Operator-selected mode at the desk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttendanceMode {
    CheckIn,
    CheckOut,
}

/// A persisted visit row (domain view).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Visit {
    pub id: i64,
    pub identity: AgentIdentity,
    /// Local ISO wall time: `YYYY-MM-DDTHH:MM:SS`.
    pub check_in_at: String,
    pub check_out_at: Option<String>,
    pub station_id: String,
    pub visit_uid: String,
}

/// Whether the agent has an open visit (check_out_at IS NULL).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisitPresence {
    /// Has open visit: currently inside.
    CheckedIn,
    /// No open visit (never visited or all visits closed).
    CheckedOut,
}

/// Outcome of one scan under a mode — drives UI message and optional persistence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanOutcome {
    CheckedIn {
        identity: AgentIdentity,
        at: String,
    },
    AlreadyCheckedIn {
        identity: AgentIdentity,
        check_in_at: String,
    },
    CheckedOut {
        identity: AgentIdentity,
        check_out_at: String,
    },
    /// Soft check-out: not locally inside; leave intent recorded for master pairing.
    OrphanCheckOut {
        identity: AgentIdentity,
        at: String,
    },
    NotCheckedIn {
        identity: AgentIdentity,
    },
    InvalidQr {
        reason: String,
    },
    EmptyInput,
    Failed {
        message: String,
    },
}

/// Command for storage to apply inside a transaction (after `decide_scan`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistCommand {
    InsertCheckIn {
        identity: AgentIdentity,
        at: String,
    },
    UpdateCheckOut {
        visit_id: i64,
        at: String,
    },
    /// Soft check-out event when agent is not open on this desk.
    InsertOrphanCheckOut {
        identity: AgentIdentity,
        at: String,
    },
}

/// Live counts for a single desk DB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AttendanceCounts {
    /// Open visits: `check_out_at IS NULL`.
    pub currently_inside: u64,
    /// Distinct `(category, license_no)` all-time in this DB file.
    pub unique_agents: u64,
    /// Total visit rows (including closed) — UI「累計人次」.
    pub total_visits: u64,
}

/// One raw visit snapshot for master rollup (from any station).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisitSnapshot {
    pub identity: AgentIdentity,
    pub check_in_at: String,
    pub check_out_at: Option<String>,
    pub station_id: String,
    pub visit_uid: String,
}

/// Orphan / event check-out for pairing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckoutEvent {
    pub identity: AgentIdentity,
    pub at: String,
    pub station_id: String,
    pub event_uid: String,
}

/// Master attendance status for one identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MasterStatus {
    /// Has check-in(s) and all paired or closed.
    Left,
    /// Still open on at least one station after pairing.
    StillInside,
    /// Check-in only (no checkout recorded).
    CheckInOnly,
    /// Orphan checkout only (no matching check-in).
    OrphanOutOnly,
}

/// One master rollup row per agent identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MasterAgentRow {
    pub identity: AgentIdentity,
    pub first_check_in_at: Option<String>,
    pub last_check_out_at: Option<String>,
    pub stations_seen: Vec<String>,
    pub visit_count: u64,
    pub open_stations: Vec<String>,
    pub status: MasterStatus,
    pub needs_review: bool,
}
