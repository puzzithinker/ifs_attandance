//! Pure domain logic for IFS event attendance (check-in / check-out).
//!
//! No I/O: QR parsing, check-in/check-out decisions, Chinese messages, master rollup.

mod cpd;
mod messages;
mod model;
mod parse;
mod rollup;
mod service;

pub use cpd::{
    format_hhmm, minutes_of_timestamp, parse_hhmm, CpdDecision, CpdError, CpdPolicy, CpdWindow,
    CPD_DEFAULT_POINTS,
};
pub use messages::{message_zh, outcome_from_parse_error};
pub use model::{
    AgentIdentity, AttendanceCounts, AttendanceMode, CheckoutEvent, MasterAgentRow, MasterStatus,
    PersistCommand, ScanOutcome, Visit, VisitPresence, VisitSnapshot,
};
pub use parse::{parse_qr_url, ParseError};
pub use rollup::{rollup_master, rollup_visits_only, unique_identity_count, PairingResult};
pub use service::decide_scan;
