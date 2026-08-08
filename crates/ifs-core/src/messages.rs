//! Chinese operator-facing status strings.

use crate::model::ScanOutcome;

/// Map a [`ScanOutcome`] to the status banner text shown to operators.
pub fn message_zh(outcome: &ScanOutcome) -> String {
    match outcome {
        ScanOutcome::CheckedIn { .. } => "已登記入場".to_string(),
        ScanOutcome::AlreadyCheckedIn { .. } => "已在場內 (重複入場)".to_string(),
        ScanOutcome::CheckedOut { .. } => "已登記離場".to_string(),
        ScanOutcome::OrphanCheckOut { .. } => "已登記離場 (跨站點)".to_string(),
        ScanOutcome::NotCheckedIn { .. } => "尚未入場，無法離場".to_string(),
        ScanOutcome::InvalidQr { .. } => "QR Code 無效".to_string(),
        ScanOutcome::EmptyInput => "請掃描 QR Code".to_string(),
        ScanOutcome::Failed { message } => format!("操作失敗: {message}"),
    }
}

/// Convenience: parse error → UI outcome (does not touch storage).
pub fn outcome_from_parse_error(err: &crate::ParseError) -> ScanOutcome {
    match err {
        crate::ParseError::Empty => ScanOutcome::EmptyInput,
        crate::ParseError::Invalid { reason } => ScanOutcome::InvalidQr {
            reason: reason.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AgentIdentity;
    use crate::ParseError;

    #[test]
    fn all_outcomes_have_non_empty_messages() {
        let id = AgentIdentity::new("IA", "1");
        let outcomes = [
            ScanOutcome::CheckedIn {
                identity: id.clone(),
                at: "t".into(),
            },
            ScanOutcome::AlreadyCheckedIn {
                identity: id.clone(),
                check_in_at: "t".into(),
            },
            ScanOutcome::CheckedOut {
                identity: id.clone(),
                check_out_at: "t".into(),
            },
            ScanOutcome::OrphanCheckOut {
                identity: id.clone(),
                at: "t".into(),
            },
            ScanOutcome::NotCheckedIn {
                identity: id.clone(),
            },
            ScanOutcome::InvalidQr {
                reason: "x".into(),
            },
            ScanOutcome::EmptyInput,
            ScanOutcome::Failed {
                message: "db".into(),
            },
        ];
        for o in &outcomes {
            let m = message_zh(o);
            assert!(!m.is_empty(), "empty message for {o:?}");
        }
    }

    #[test]
    fn exact_copy_for_happy_paths() {
        let id = AgentIdentity::new("IA", "1");
        assert_eq!(
            message_zh(&ScanOutcome::CheckedIn {
                identity: id.clone(),
                at: "t".into(),
            }),
            "已登記入場"
        );
        assert_eq!(
            message_zh(&ScanOutcome::AlreadyCheckedIn {
                identity: id.clone(),
                check_in_at: "t".into(),
            }),
            "已在場內 (重複入場)"
        );
        assert_eq!(
            message_zh(&ScanOutcome::CheckedOut {
                identity: id.clone(),
                check_out_at: "t".into(),
            }),
            "已登記離場"
        );
        assert_eq!(
            message_zh(&ScanOutcome::OrphanCheckOut {
                identity: id.clone(),
                at: "t".into(),
            }),
            "已登記離場 (跨站點)"
        );
        assert_eq!(
            message_zh(&ScanOutcome::NotCheckedIn { identity: id }),
            "尚未入場，無法離場"
        );
        assert_eq!(message_zh(&ScanOutcome::EmptyInput), "請掃描 QR Code");
        assert_eq!(
            message_zh(&ScanOutcome::InvalidQr {
                reason: "missing".into(),
            }),
            "QR Code 無效"
        );
        assert_eq!(
            message_zh(&ScanOutcome::Failed {
                message: "locked".into(),
            }),
            "操作失敗: locked"
        );
    }

    #[test]
    fn parse_error_mapping() {
        assert_eq!(
            outcome_from_parse_error(&ParseError::Empty),
            ScanOutcome::EmptyInput
        );
        assert_eq!(
            outcome_from_parse_error(&ParseError::Invalid {
                reason: "missing or empty categoryCode".into(),
            }),
            ScanOutcome::InvalidQr {
                reason: "missing or empty categoryCode".into(),
            }
        );
    }
}
