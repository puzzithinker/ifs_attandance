//! Pure scan feedback model — outcome to tone/headline/detail mapping.
//! No egui widget code here; unit-tested for every ScanOutcome variant.

use crate::theme::StatusTone;
use ifs_core::{AttendanceMode, ScanOutcome};

/// One scan result rendered as UI feedback (headline + subject + detail + tone).
#[derive(Clone)]
pub(crate) struct ScanFeedback {
    pub(crate) headline: String,
    pub(crate) subject: String,
    pub(crate) detail: String,
    pub(crate) tone: StatusTone,
    pub(crate) at: String,
}

pub(crate) fn tone_for_outcome(outcome: &ScanOutcome) -> StatusTone {
    match outcome {
        ScanOutcome::CheckedIn { .. } | ScanOutcome::CheckedOut { .. } => StatusTone::Success,
        ScanOutcome::OrphanCheckOut { .. } => StatusTone::Info,
        ScanOutcome::AlreadyCheckedIn { .. } | ScanOutcome::NotCheckedIn { .. } => {
            StatusTone::Warning
        }
        ScanOutcome::InvalidQr { .. } | ScanOutcome::Failed { .. } => StatusTone::Error,
        ScanOutcome::EmptyInput => StatusTone::Neutral,
    }
}

pub(crate) fn headline_for_outcome(outcome: &ScanOutcome) -> String {
    match outcome {
        ScanOutcome::CheckedIn { .. } => "入場成功".into(),
        ScanOutcome::AlreadyCheckedIn { .. } => "重複入場".into(),
        ScanOutcome::CheckedOut { .. } => "離場成功".into(),
        ScanOutcome::OrphanCheckOut { .. } => "跨站點離場".into(),
        ScanOutcome::NotCheckedIn { .. } => "無法離場".into(),
        ScanOutcome::InvalidQr { .. } => "QR 無效".into(),
        ScanOutcome::EmptyInput => "未輸入".into(),
        ScanOutcome::Failed { .. } => "操作失敗".into(),
    }
}

pub(crate) fn detail_for_outcome(outcome: &ScanOutcome, mode: AttendanceMode) -> String {
    let mode_s = match mode {
        AttendanceMode::CheckIn => "模式：入場",
        AttendanceMode::CheckOut => "模式：離場",
    };
    match outcome {
        ScanOutcome::CheckedIn { at, .. } => format!("{mode_s} · 登記時間 {at}"),
        ScanOutcome::AlreadyCheckedIn { check_in_at, .. } => {
            format!("{mode_s} · 本機已於 {check_in_at} 入場（未重複寫入）")
        }
        ScanOutcome::CheckedOut { check_out_at, .. } => {
            format!("{mode_s} · 離場時間 {check_out_at}")
        }
        ScanOutcome::OrphanCheckOut { at, .. } => {
            format!("{mode_s} · 已記錄離場意圖 {at}（入場可能在其他站點；主控合併時配對）")
        }
        ScanOutcome::NotCheckedIn { .. } => format!("{mode_s} · 本機無未離場記錄"),
        ScanOutcome::InvalidQr { reason } => format!("原因：{reason}"),
        ScanOutcome::EmptyInput => "請掃描或貼上 QR URL 後按 Enter".into(),
        ScanOutcome::Failed { message } => format!("錯誤：{message}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ifs_core::AgentIdentity;

    fn id() -> AgentIdentity {
        AgentIdentity::new("IA", "123")
    }

    #[test]
    fn checked_in_maps_to_success() {
        let o = ScanOutcome::CheckedIn {
            identity: id(),
            at: "2026-08-08T09:00:00".into(),
        };
        assert_eq!(tone_for_outcome(&o), StatusTone::Success);
        assert_eq!(headline_for_outcome(&o), "入場成功");
        assert!(detail_for_outcome(&o, AttendanceMode::CheckIn).contains("登記時間"));
    }

    #[test]
    fn already_checked_in_maps_to_warning() {
        let o = ScanOutcome::AlreadyCheckedIn {
            identity: id(),
            check_in_at: "2026-08-08T09:00:00".into(),
        };
        assert_eq!(tone_for_outcome(&o), StatusTone::Warning);
        assert_eq!(headline_for_outcome(&o), "重複入場");
        assert!(detail_for_outcome(&o, AttendanceMode::CheckIn).contains("未重複寫入"));
    }

    #[test]
    fn checked_out_maps_to_success() {
        let o = ScanOutcome::CheckedOut {
            identity: id(),
            check_out_at: "2026-08-08T12:00:00".into(),
        };
        assert_eq!(tone_for_outcome(&o), StatusTone::Success);
        assert_eq!(headline_for_outcome(&o), "離場成功");
        assert!(detail_for_outcome(&o, AttendanceMode::CheckOut).contains("離場時間"));
    }

    #[test]
    fn orphan_check_out_maps_to_info() {
        let o = ScanOutcome::OrphanCheckOut {
            identity: id(),
            at: "2026-08-08T12:00:00".into(),
        };
        assert_eq!(tone_for_outcome(&o), StatusTone::Info);
        assert_eq!(headline_for_outcome(&o), "跨站點離場");
        assert!(detail_for_outcome(&o, AttendanceMode::CheckOut).contains("離場意圖"));
    }

    #[test]
    fn not_checked_in_maps_to_warning() {
        let o = ScanOutcome::NotCheckedIn { identity: id() };
        assert_eq!(tone_for_outcome(&o), StatusTone::Warning);
        assert_eq!(headline_for_outcome(&o), "無法離場");
        assert!(detail_for_outcome(&o, AttendanceMode::CheckOut).contains("無未離場"));
    }

    #[test]
    fn invalid_qr_maps_to_error() {
        let o = ScanOutcome::InvalidQr {
            reason: "bad url".into(),
        };
        assert_eq!(tone_for_outcome(&o), StatusTone::Error);
        assert_eq!(headline_for_outcome(&o), "QR 無效");
        assert!(detail_for_outcome(&o, AttendanceMode::CheckIn).contains("原因"));
    }

    #[test]
    fn empty_input_maps_to_neutral() {
        let o = ScanOutcome::EmptyInput;
        assert_eq!(tone_for_outcome(&o), StatusTone::Neutral);
        assert_eq!(headline_for_outcome(&o), "未輸入");
        assert!(detail_for_outcome(&o, AttendanceMode::CheckIn).contains("Enter"));
    }

    #[test]
    fn failed_maps_to_error() {
        let o = ScanOutcome::Failed {
            message: "db locked".into(),
        };
        assert_eq!(tone_for_outcome(&o), StatusTone::Error);
        assert_eq!(headline_for_outcome(&o), "操作失敗");
        assert!(detail_for_outcome(&o, AttendanceMode::CheckIn).contains("錯誤"));
    }

    #[test]
    fn detail_includes_mode_label() {
        let o = ScanOutcome::CheckedIn {
            identity: id(),
            at: "2026-08-08T09:00:00".into(),
        };
        assert!(detail_for_outcome(&o, AttendanceMode::CheckIn).contains("模式：入場"));
        assert!(detail_for_outcome(&o, AttendanceMode::CheckOut).contains("模式：離場"));
    }
}
