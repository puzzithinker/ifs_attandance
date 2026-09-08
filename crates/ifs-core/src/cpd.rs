//! CPD eligibility: per-event time windows for check-in / check-out.
//!
//! An event may define a check-in window (e.g. `14:30`–`15:00`) and a
//! check-out window (e.g. `17:10`–`17:30`). A visit earns the event's CPD
//! points when its check-in falls inside the check-in window **and** its
//! check-out falls inside the check-out window.
//!
//! All comparisons are time-of-day, minute-granular, and inclusive on both
//! bounds. Unset bounds are unbounded. Windows live in the event settings
//! (desk `app_meta`) and appear in the exported CSVs as the `CPD` column:
//! points when earned, `0` when missed, blank when no policy is configured.

use thiserror::Error;

/// Minutes since midnight; inclusive window bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpdWindow {
    /// Earliest allowed minute (inclusive); `None` = no lower bound.
    pub from: Option<u32>,
    /// Latest allowed minute (inclusive); `None` = no upper bound.
    pub until: Option<u32>,
}

impl CpdWindow {
    /// Minute is inside the window (inclusive bounds, unset = unbounded).
    pub fn contains(&self, minute: u32) -> bool {
        if let Some(from) = self.from {
            if minute < from {
                return false;
            }
        }
        if let Some(until) = self.until {
            if minute > until {
                return false;
            }
        }
        true
    }
}

/// One event's CPD rule set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpdPolicy {
    pub check_in: CpdWindow,
    pub check_out: CpdWindow,
    /// CPD points granted for a qualifying visit.
    pub points: u32,
}

/// Whether a visit qualifies for CPD under a [`CpdPolicy`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpdDecision {
    /// Check-in and check-out both inside their windows.
    Earned,
    /// Missing check-out, or either time outside its window.
    Missed,
}

/// Errors from parsing CPD settings strings.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CpdError {
    #[error("check-in from: {0}")]
    CheckInFrom(String),
    #[error("check-in until: {0}")]
    CheckInUntil(String),
    #[error("check-out from: {0}")]
    CheckOutFrom(String),
    #[error("check-out until: {0}")]
    CheckOutUntil(String),
    #[error("CPD points: {0}")]
    Points(String),
}

/// Default points when a policy is configured but points left blank.
pub const CPD_DEFAULT_POINTS: u32 = 2;

/// Parse a settings time string. Blank = unbounded (`None`).
/// Accepts `H:MM` / `HH:MM` (optionally with `:SS`, which is ignored).
pub fn parse_hhmm(raw: &str) -> Result<Option<u32>, String> {
    let s = raw.trim();
    if s.is_empty() {
        return Ok(None);
    }
    let mut parts = s.split(':');
    let bad = || format!("expected HH:MM, got \"{s}\"");
    let h: u32 = parts.next().ok_or_else(bad)?.parse().map_err(|_| bad())?;
    let m: u32 = parts.next().ok_or_else(bad)?.parse().map_err(|_| bad())?;
    if parts.next().is_some_and(|sec| sec.parse::<u32>().is_err()) {
        return Err(bad());
    }
    if h > 23 || m > 59 {
        return Err(format!("\"{s}\" is not a valid time of day"));
    }
    Ok(Some(h * 60 + m))
}

/// Minute-of-day of a stored local timestamp `YYYY-MM-DDTHH:MM[:SS]`.
/// Returns `None` for malformed values.
pub fn minutes_of_timestamp(iso: &str) -> Option<u32> {
    let time = iso.trim().split('T').nth(1)?;
    let mut parts = time.split(':');
    let h: u32 = parts.next()?.parse().ok()?;
    let m: u32 = parts.next()?.parse().ok()?;
    if h > 23 || m > 59 {
        return None;
    }
    Some(h * 60 + m)
}

/// `HH:MM` display for minutes-since-midnight.
pub fn format_hhmm(minute: u32) -> String {
    format!("{:02}:{:02}", minute / 60, minute % 60)
}

impl CpdPolicy {
    /// Build a policy from raw settings strings (blank = unset).
    ///
    /// * All five blank → `Ok(None)` — no CPD policy, CSV column stays blank.
    /// * Points blank (with any bound set) → [`CPD_DEFAULT_POINTS`].
    /// * Any bound set to a later time than its pair → error (no overnight
    ///   windows; reorder the bounds instead).
    pub fn from_config(
        check_in_from: &str,
        check_in_until: &str,
        check_out_from: &str,
        check_out_until: &str,
        points: &str,
    ) -> Result<Option<CpdPolicy>, CpdError> {
        let blank = |s: &str| s.trim().is_empty();
        if [check_in_from, check_in_until, check_out_from, check_out_until, points]
            .iter()
            .all(|s| blank(s))
        {
            return Ok(None);
        }

        let in_from = parse_hhmm(check_in_from).map_err(CpdError::CheckInFrom)?;
        let in_until = parse_hhmm(check_in_until).map_err(CpdError::CheckInUntil)?;
        let out_from = parse_hhmm(check_out_from).map_err(CpdError::CheckOutFrom)?;
        let out_until = parse_hhmm(check_out_until).map_err(CpdError::CheckOutUntil)?;

        if let (Some(from), Some(until)) = (in_from, in_until) {
            if from > until {
                return Err(CpdError::CheckInFrom(format!(
                    "window start {} is after window end {}",
                    format_hhmm(from),
                    format_hhmm(until)
                )));
            }
        }
        if let (Some(from), Some(until)) = (out_from, out_until) {
            if from > until {
                return Err(CpdError::CheckOutFrom(format!(
                    "window start {} is after window end {}",
                    format_hhmm(from),
                    format_hhmm(until)
                )));
            }
        }

        let points = if blank(points) {
            CPD_DEFAULT_POINTS
        } else {
            points
                .trim()
                .parse::<u32>()
                .map_err(|_| CpdError::Points(format!("expected a whole number, got \"{}\"", points.trim())))?
        };

        Ok(Some(CpdPolicy {
            check_in: CpdWindow {
                from: in_from,
                until: in_until,
            },
            check_out: CpdWindow {
                from: out_from,
                until: out_until,
            },
            points,
        }))
    }

    /// Did this visit earn CPD? Unparseable timestamps never earn.
    pub fn decide(&self, check_in_at: &str, check_out_at: Option<&str>) -> CpdDecision {
        let Some(in_min) = minutes_of_timestamp(check_in_at) else {
            return CpdDecision::Missed;
        };
        if !self.check_in.contains(in_min) {
            return CpdDecision::Missed;
        }
        let Some(check_out_at) = check_out_at else {
            return CpdDecision::Missed;
        };
        let Some(out_min) = minutes_of_timestamp(check_out_at) else {
            return CpdDecision::Missed;
        };
        if !self.check_out.contains(out_min) {
            return CpdDecision::Missed;
        }
        CpdDecision::Earned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IN: &str = "14:30";
    const IN_END: &str = "15:00";
    const OUT: &str = "17:10";
    const OUT_END: &str = "17:30";

    fn policy() -> CpdPolicy {
        CpdPolicy::from_config(IN, IN_END, OUT, OUT_END, "2")
            .unwrap()
            .expect("configured")
    }

    #[test]
    fn parse_hhmm_accepts_blank_and_hmm_variants() {
        assert_eq!(parse_hhmm("").unwrap(), None);
        assert_eq!(parse_hhmm("  ").unwrap(), None);
        assert_eq!(parse_hhmm("14:30").unwrap(), Some(14 * 60 + 30));
        assert_eq!(parse_hhmm("9:05").unwrap(), Some(9 * 60 + 5));
        assert_eq!(parse_hhmm("17:30:45").unwrap(), Some(17 * 60 + 30));
        assert!(parse_hhmm("24:00").is_err());
        assert!(parse_hhmm("12:60").is_err());
        assert!(parse_hhmm("14e30").is_err());
        assert!(parse_hhmm("ab:cd").is_err());
        assert!(parse_hhmm("14").is_err());
    }

    #[test]
    fn all_blank_config_is_no_policy() {
        assert_eq!(CpdPolicy::from_config("", "", "", "", "").unwrap(), None);
    }

    #[test]
    fn blank_points_default_to_two() {
        let p = CpdPolicy::from_config(IN, IN_END, "", "", "")
            .unwrap()
            .expect("configured");
        assert_eq!(p.points, CPD_DEFAULT_POINTS);
        assert_eq!(p.check_out.from, None);
        assert_eq!(p.check_out.until, None);
    }

    #[test]
    fn points_alone_is_a_policy() {
        let p = CpdPolicy::from_config("", "", "", "", "3")
            .unwrap()
            .expect("configured");
        assert_eq!(p.points, 3);
        assert_eq!(p.check_in, CpdWindow { from: None, until: None });
    }

    #[test]
    fn invalid_bound_names_its_field() {
        let e = CpdPolicy::from_config(" banana", "", "", "", "").unwrap_err();
        assert_eq!(e, CpdError::CheckInFrom("expected HH:MM, got \"banana\"".into()));
        let e = CpdPolicy::from_config("", "", "", "25:00", "").unwrap_err();
        assert!(matches!(e, CpdError::CheckOutUntil(_)));
        let e = CpdPolicy::from_config("", "", "", "", "two").unwrap_err();
        assert!(matches!(e, CpdError::Points(_)));
    }

    #[test]
    fn reversed_window_is_rejected() {
        assert!(CpdPolicy::from_config("15:00", "14:30", "", "", "").is_err());
        assert!(CpdPolicy::from_config("", "", "17:30", "17:10", "").is_err());
    }

    #[test]
    fn full_policy_parses_windows_and_points() {
        let p = policy();
        assert_eq!(p.check_in.from, Some(870));
        assert_eq!(p.check_in.until, Some(900));
        assert_eq!(p.check_out.from, Some(1030));
        assert_eq!(p.check_out.until, Some(1050));
        assert_eq!(p.points, 2);
    }

    #[test]
    fn decide_uses_inclusive_bounds() {
        let p = policy();
        // Exactly on both edges: earned.
        assert_eq!(
            p.decide("2026-09-10T14:30:00", Some("2026-09-10T17:30:59")),
            CpdDecision::Earned
        );
        // One minute late on check-in.
        assert_eq!(
            p.decide("2026-09-10T15:01:00", Some("2026-09-10T17:20:00")),
            CpdDecision::Missed
        );
        // One minute early on check-out.
        assert_eq!(
            p.decide("2026-09-10T14:40:00", Some("2026-09-10T17:09:00")),
            CpdDecision::Missed
        );
        // Left too late.
        assert_eq!(
            p.decide("2026-09-10T14:40:00", Some("2026-09-10T17:31:00")),
            CpdDecision::Missed
        );
    }

    #[test]
    fn decide_requires_check_out() {
        let p = policy();
        assert_eq!(p.decide("2026-09-10T14:40:00", None), CpdDecision::Missed);
    }

    #[test]
    fn decide_ignores_date_part() {
        let p = policy();
        // Multi-day dataset: each visit judged on its own time of day.
        assert_eq!(
            p.decide("2026-09-11T14:45:00", Some("2026-09-11T17:15:00")),
            CpdDecision::Earned
        );
    }

    #[test]
    fn unbounded_sides_accept_any_time() {
        // Only check-out window configured.
        let p = CpdPolicy::from_config("", "", OUT, OUT_END, "2")
            .unwrap()
            .expect("configured");
        assert_eq!(
            p.decide("2026-09-10T09:00:00", Some("2026-09-10T17:15:00")),
            CpdDecision::Earned
        );
        assert_eq!(
            p.decide("2026-09-10T09:00:00", Some("2026-09-10T18:15:00")),
            CpdDecision::Missed
        );
    }

    #[test]
    fn malformed_timestamps_never_earn() {
        let p = CpdPolicy::from_config("", "", "", "", "2").unwrap().expect("configured");
        assert_eq!(p.decide("not-a-timestamp", Some("2026-09-10T17:15:00")), CpdDecision::Missed);
        assert_eq!(p.decide("", None), CpdDecision::Missed);
    }
}
