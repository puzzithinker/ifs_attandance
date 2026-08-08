//! Pure check-in / check-out state machine.

use crate::model::{
    AgentIdentity, AttendanceMode, PersistCommand, ScanOutcome, VisitPresence,
};

/// Pure state transition for one scan under the selected mode.
///
/// Does **not** parse QR input — caller must supply a valid [`AgentIdentity`].
///
/// * `soft_checkout` — when true and CheckOut while Outside, record orphan leave
///   intent instead of only `NotCheckedIn`.
pub fn decide_scan(
    mode: AttendanceMode,
    presence: VisitPresence,
    identity: AgentIdentity,
    now_iso: &str,
    open_visit_id: Option<i64>,
    open_check_in_at: Option<&str>,
    soft_checkout: bool,
) -> (ScanOutcome, Option<PersistCommand>) {
    match (mode, presence) {
        (AttendanceMode::CheckIn, VisitPresence::CheckedOut) => {
            let at = now_iso.to_string();
            let outcome = ScanOutcome::CheckedIn {
                identity: identity.clone(),
                at: at.clone(),
            };
            let cmd = PersistCommand::InsertCheckIn { identity, at };
            (outcome, Some(cmd))
        }
        (AttendanceMode::CheckIn, VisitPresence::CheckedIn) => {
            let check_in_at = open_check_in_at.unwrap_or("").to_string();
            (
                ScanOutcome::AlreadyCheckedIn {
                    identity,
                    check_in_at,
                },
                None,
            )
        }
        (AttendanceMode::CheckOut, VisitPresence::CheckedIn) => {
            let Some(visit_id) = open_visit_id else {
                return (
                    ScanOutcome::Failed {
                        message: "open visit missing id".to_string(),
                    },
                    None,
                );
            };
            let at = now_iso.to_string();
            let outcome = ScanOutcome::CheckedOut {
                identity,
                check_out_at: at.clone(),
            };
            let cmd = PersistCommand::UpdateCheckOut { visit_id, at };
            (outcome, Some(cmd))
        }
        (AttendanceMode::CheckOut, VisitPresence::CheckedOut) => {
            if soft_checkout {
                let at = now_iso.to_string();
                let outcome = ScanOutcome::OrphanCheckOut {
                    identity: identity.clone(),
                    at: at.clone(),
                };
                let cmd = PersistCommand::InsertOrphanCheckOut { identity, at };
                (outcome, Some(cmd))
            } else {
                (ScanOutcome::NotCheckedIn { identity }, None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent() -> AgentIdentity {
        AgentIdentity::new("IA", "12345")
    }

    const NOW: &str = "2026-08-08T10:00:00";

    #[test]
    fn check_in_when_outside_inserts() {
        let (outcome, cmd) = decide_scan(
            AttendanceMode::CheckIn,
            VisitPresence::CheckedOut,
            agent(),
            NOW,
            None,
            None,
            false,
        );
        assert_eq!(
            outcome,
            ScanOutcome::CheckedIn {
                identity: agent(),
                at: NOW.to_string(),
            }
        );
        assert_eq!(
            cmd,
            Some(PersistCommand::InsertCheckIn {
                identity: agent(),
                at: NOW.to_string(),
            })
        );
    }

    #[test]
    fn check_in_when_inside_is_duplicate() {
        let (outcome, cmd) = decide_scan(
            AttendanceMode::CheckIn,
            VisitPresence::CheckedIn,
            agent(),
            NOW,
            Some(7),
            Some("2026-08-08T09:00:00"),
            false,
        );
        assert_eq!(
            outcome,
            ScanOutcome::AlreadyCheckedIn {
                identity: agent(),
                check_in_at: "2026-08-08T09:00:00".to_string(),
            }
        );
        assert_eq!(cmd, None);
    }

    #[test]
    fn check_out_when_inside_updates() {
        let (outcome, cmd) = decide_scan(
            AttendanceMode::CheckOut,
            VisitPresence::CheckedIn,
            agent(),
            NOW,
            Some(42),
            Some("2026-08-08T09:00:00"),
            false,
        );
        assert_eq!(
            outcome,
            ScanOutcome::CheckedOut {
                identity: agent(),
                check_out_at: NOW.to_string(),
            }
        );
        assert_eq!(
            cmd,
            Some(PersistCommand::UpdateCheckOut {
                visit_id: 42,
                at: NOW.to_string(),
            })
        );
    }

    #[test]
    fn check_out_when_outside_not_checked_in() {
        let (outcome, cmd) = decide_scan(
            AttendanceMode::CheckOut,
            VisitPresence::CheckedOut,
            agent(),
            NOW,
            None,
            None,
            false,
        );
        assert_eq!(outcome, ScanOutcome::NotCheckedIn { identity: agent() });
        assert_eq!(cmd, None);
    }

    #[test]
    fn soft_check_out_when_outside_records_orphan() {
        let (outcome, cmd) = decide_scan(
            AttendanceMode::CheckOut,
            VisitPresence::CheckedOut,
            agent(),
            NOW,
            None,
            None,
            true,
        );
        assert_eq!(
            outcome,
            ScanOutcome::OrphanCheckOut {
                identity: agent(),
                at: NOW.to_string(),
            }
        );
        assert_eq!(
            cmd,
            Some(PersistCommand::InsertOrphanCheckOut {
                identity: agent(),
                at: NOW.to_string(),
            })
        );
    }

    #[test]
    fn check_out_inside_without_visit_id_fails() {
        let (outcome, cmd) = decide_scan(
            AttendanceMode::CheckOut,
            VisitPresence::CheckedIn,
            agent(),
            NOW,
            None,
            Some("2026-08-08T09:00:00"),
            false,
        );
        assert!(matches!(outcome, ScanOutcome::Failed { .. }));
        assert_eq!(cmd, None);
    }

    #[test]
    fn re_entry_is_check_in_when_outside() {
        let (outcome, cmd) = decide_scan(
            AttendanceMode::CheckIn,
            VisitPresence::CheckedOut,
            agent(),
            "2026-08-08T14:00:00",
            None,
            None,
            false,
        );
        assert!(matches!(outcome, ScanOutcome::CheckedIn { .. }));
        assert!(matches!(cmd, Some(PersistCommand::InsertCheckIn { .. })));
    }

    #[test]
    fn full_mode_matrix() {
        let cases = [
            (
                AttendanceMode::CheckIn,
                VisitPresence::CheckedOut,
                true,
            ),
            (AttendanceMode::CheckIn, VisitPresence::CheckedIn, false),
            (AttendanceMode::CheckOut, VisitPresence::CheckedIn, true),
            (AttendanceMode::CheckOut, VisitPresence::CheckedOut, false),
        ];
        for (mode, presence, expect_cmd) in cases {
            let open_id = if presence == VisitPresence::CheckedIn {
                Some(1)
            } else {
                None
            };
            let open_at = if presence == VisitPresence::CheckedIn {
                Some("2026-08-08T08:00:00")
            } else {
                None
            };
            let (_o, cmd) = decide_scan(mode, presence, agent(), NOW, open_id, open_at, false);
            assert_eq!(cmd.is_some(), expect_cmd, "mode={mode:?} presence={presence:?}");
        }
    }

    #[test]
    fn soft_checkout_does_not_change_check_in_path() {
        let (o, cmd) = decide_scan(
            AttendanceMode::CheckIn,
            VisitPresence::CheckedOut,
            agent(),
            NOW,
            None,
            None,
            true,
        );
        assert!(matches!(o, ScanOutcome::CheckedIn { .. }));
        assert!(matches!(cmd, Some(PersistCommand::InsertCheckIn { .. })));
    }

    #[test]
    fn soft_checkout_does_not_change_local_checkout() {
        let (o, cmd) = decide_scan(
            AttendanceMode::CheckOut,
            VisitPresence::CheckedIn,
            agent(),
            NOW,
            Some(9),
            Some("2026-08-08T08:00:00"),
            true,
        );
        assert!(matches!(o, ScanOutcome::CheckedOut { .. }));
        assert_eq!(
            cmd,
            Some(PersistCommand::UpdateCheckOut {
                visit_id: 9,
                at: NOW.to_string(),
            })
        );
    }

    #[test]
    fn already_checked_in_uses_empty_string_if_open_at_missing() {
        let (o, cmd) = decide_scan(
            AttendanceMode::CheckIn,
            VisitPresence::CheckedIn,
            agent(),
            NOW,
            Some(1),
            None,
            false,
        );
        assert_eq!(
            o,
            ScanOutcome::AlreadyCheckedIn {
                identity: agent(),
                check_in_at: String::new(),
            }
        );
        assert!(cmd.is_none());
    }

    #[test]
    fn insert_check_in_clones_identity_into_command() {
        let id = AgentIdentity::new("BR", "ZZ");
        let (o, cmd) = decide_scan(
            AttendanceMode::CheckIn,
            VisitPresence::CheckedOut,
            id.clone(),
            "2026-01-01T00:00:00",
            None,
            None,
            false,
        );
        match (o, cmd) {
            (
                ScanOutcome::CheckedIn { identity: i1, at: a1 },
                Some(PersistCommand::InsertCheckIn {
                    identity: i2,
                    at: a2,
                }),
            ) => {
                assert_eq!(i1, id);
                assert_eq!(i2, id);
                assert_eq!(a1, a2);
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
