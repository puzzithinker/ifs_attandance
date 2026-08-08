//! Master rollup and check-in/out pairing (pure, no I/O).

use crate::model::{
    AgentIdentity, CheckoutEvent, MasterAgentRow, MasterStatus, VisitSnapshot,
};
use std::collections::{BTreeMap, BTreeSet};

/// Greedy pair open check-ins with check-outs per identity.
///
/// For each identity:
/// - opens = visits sorted by check_in_at (each visit is one open interval)
/// - closes = visit check_outs + orphan checkout events, sorted by time
/// - each close matches earliest unmatched open with open_at <= close_at
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingResult {
    pub rows: Vec<MasterAgentRow>,
}

/// Build master attendance rows from all station visits and orphan check-outs.
pub fn rollup_master(
    visits: &[VisitSnapshot],
    orphan_checkouts: &[CheckoutEvent],
) -> PairingResult {
    let mut by_id: BTreeMap<(String, String), AgentBucket> = BTreeMap::new();

    for v in visits {
        let key = (v.identity.category.clone(), v.identity.license_no.clone());
        let b = by_id.entry(key).or_insert_with(|| AgentBucket {
            identity: v.identity.clone(),
            visits: Vec::new(),
            orphans: Vec::new(),
        });
        b.visits.push(v.clone());
    }
    for o in orphan_checkouts {
        let key = (o.identity.category.clone(), o.identity.license_no.clone());
        let b = by_id.entry(key).or_insert_with(|| AgentBucket {
            identity: o.identity.clone(),
            visits: Vec::new(),
            orphans: Vec::new(),
        });
        b.orphans.push(o.clone());
    }

    let mut rows: Vec<MasterAgentRow> = by_id
        .into_values()
        .map(|b| pair_agent(b))
        .collect();
    rows.sort_by(|a, b| {
        a.identity
            .category
            .cmp(&b.identity.category)
            .then_with(|| a.identity.license_no.cmp(&b.identity.license_no))
    });
    PairingResult { rows }
}

struct AgentBucket {
    identity: AgentIdentity,
    visits: Vec<VisitSnapshot>,
    orphans: Vec<CheckoutEvent>,
}

fn pair_agent(bucket: AgentBucket) -> MasterAgentRow {
    let mut stations: BTreeSet<String> = BTreeSet::new();
    for v in &bucket.visits {
        if !v.station_id.is_empty() {
            stations.insert(v.station_id.clone());
        }
    }
    for o in &bucket.orphans {
        if !o.station_id.is_empty() {
            stations.insert(o.station_id.clone());
        }
    }

    let visit_count = bucket.visits.len() as u64;
    let first_check_in_at = bucket
        .visits
        .iter()
        .map(|v| v.check_in_at.as_str())
        .min()
        .map(|s| s.to_string());

    // Build closes: from closed visits + orphans
    let mut opens: Vec<(String, String, Option<String>)> = bucket
        .visits
        .iter()
        .map(|v| {
            (
                v.check_in_at.clone(),
                v.station_id.clone(),
                v.check_out_at.clone(),
            )
        })
        .collect();
    opens.sort_by(|a, b| a.0.cmp(&b.0));

    let mut closes: Vec<(String, String)> = Vec::new();
    for v in &bucket.visits {
        if let Some(ref out) = v.check_out_at {
            closes.push((out.clone(), v.station_id.clone()));
        }
    }
    for o in &bucket.orphans {
        closes.push((o.at.clone(), o.station_id.clone()));
    }
    closes.sort_by(|a, b| a.0.cmp(&b.0));

    // Visits that already have check_out_at are pre-paired locally.
    // Track which open slots still need pairing (open visits without checkout).
    let mut unmatched_opens: Vec<(String, String)> = opens
        .iter()
        .filter(|(_, _, out)| out.is_none())
        .map(|(inn, st, _)| (inn.clone(), st.clone()))
        .collect();
    unmatched_opens.sort_by(|a, b| a.0.cmp(&b.0));

    // Orphan closes + we already used visit-local checkouts as closed.
    // Only orphan events need pairing to open visits.
    let mut orphan_closes: Vec<(String, String)> = bucket
        .orphans
        .iter()
        .map(|o| (o.at.clone(), o.station_id.clone()))
        .collect();
    orphan_closes.sort_by(|a, b| a.0.cmp(&b.0));

    let mut paired_outs: Vec<String> = opens
        .iter()
        .filter_map(|(_, _, out)| out.clone())
        .collect();

    let mut unmatched_orphan_closes = 0u64;
    let mut skew_flags = 0u64;

    for (close_at, _st) in &orphan_closes {
        if let Some(idx) = unmatched_opens
            .iter()
            .position(|(open_at, _)| open_at.as_str() <= close_at.as_str())
        {
            paired_outs.push(close_at.clone());
            unmatched_opens.remove(idx);
        } else if unmatched_opens.iter().any(|(open_at, _)| open_at > close_at) {
            // out before any remaining open
            skew_flags += 1;
            unmatched_orphan_closes += 1;
        } else {
            unmatched_orphan_closes += 1;
        }
    }

    let open_stations: Vec<String> = {
        let mut s: BTreeSet<String> = BTreeSet::new();
        for (_, st) in &unmatched_opens {
            if !st.is_empty() {
                s.insert(st.clone());
            }
        }
        s.into_iter().collect()
    };

    let last_check_out_at = paired_outs.iter().max().cloned();

    let (status, needs_review) = if first_check_in_at.is_none() && !bucket.orphans.is_empty() {
        (MasterStatus::OrphanOutOnly, true)
    } else if !unmatched_opens.is_empty() {
        let review = unmatched_opens.len() > 1
            || unmatched_orphan_closes > 0
            || skew_flags > 0
            || open_stations.len() > 1;
        (MasterStatus::StillInside, review || open_stations.len() > 1)
    } else if last_check_out_at.is_some() {
        let review = unmatched_orphan_closes > 0 || skew_flags > 0;
        (MasterStatus::Left, review)
    } else if first_check_in_at.is_some() {
        (MasterStatus::CheckInOnly, false)
    } else {
        (MasterStatus::OrphanOutOnly, true)
    };

    // Dual open stations always needs_review
    let needs_review = needs_review || open_stations.len() > 1 || unmatched_orphan_closes > 0;

    MasterAgentRow {
        identity: bucket.identity,
        first_check_in_at,
        last_check_out_at: if matches!(status, MasterStatus::StillInside) {
            // Design: empty if any open remains
            None
        } else {
            last_check_out_at
        },
        stations_seen: stations.into_iter().collect(),
        visit_count,
        open_stations,
        status,
        needs_review,
    }
}

/// Convenience: rollup from visits only (no orphans).
pub fn rollup_visits_only(visits: &[VisitSnapshot]) -> PairingResult {
    rollup_master(visits, &[])
}

/// Group count of unique identities.
pub fn unique_identity_count(rows: &[MasterAgentRow]) -> u64 {
    rows.len() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(c: &str, l: &str) -> AgentIdentity {
        AgentIdentity::new(c, l)
    }

    fn visit(cat: &str, lic: &str, inn: &str, out: Option<&str>, st: &str, uid: &str) -> VisitSnapshot {
        VisitSnapshot {
            identity: id(cat, lic),
            check_in_at: inn.into(),
            check_out_at: out.map(|s| s.into()),
            station_id: st.into(),
            visit_uid: uid.into(),
        }
    }

    #[test]
    fn two_stations_same_agent_dedupe() {
        let visits = [
            visit("IA", "1", "2026-08-08T09:00:00", None, "S1", "u1"),
            visit("IA", "1", "2026-08-08T10:00:00", None, "S2", "u2"),
        ];
        let r = rollup_master(&visits, &[]);
        assert_eq!(r.rows.len(), 1);
        assert_eq!(r.rows[0].first_check_in_at.as_deref(), Some("2026-08-08T09:00:00"));
        assert_eq!(r.rows[0].visit_count, 2);
        assert_eq!(r.rows[0].stations_seen.len(), 2);
        assert_eq!(r.rows[0].status, MasterStatus::StillInside);
        assert!(r.rows[0].needs_review);
    }

    #[test]
    fn cross_station_soft_checkout_pairs() {
        let visits = [visit(
            "IA",
            "9",
            "2026-08-08T10:00:00",
            None,
            "S1",
            "v-in",
        )];
        let orphans = [CheckoutEvent {
            identity: id("IA", "9"),
            at: "2026-08-08T12:00:00".into(),
            station_id: "S2".into(),
            event_uid: "e-out".into(),
        }];
        let r = rollup_master(&visits, &orphans);
        assert_eq!(r.rows.len(), 1);
        let row = &r.rows[0];
        assert_eq!(row.first_check_in_at.as_deref(), Some("2026-08-08T10:00:00"));
        assert_eq!(row.last_check_out_at.as_deref(), Some("2026-08-08T12:00:00"));
        assert_eq!(row.status, MasterStatus::Left);
        assert!(row.open_stations.is_empty());
    }

    #[test]
    fn orphan_out_only_needs_review() {
        let orphans = [CheckoutEvent {
            identity: id("BR", "2"),
            at: "2026-08-08T12:00:00".into(),
            station_id: "S2".into(),
            event_uid: "e1".into(),
        }];
        let r = rollup_master(&[], &orphans);
        assert_eq!(r.rows[0].status, MasterStatus::OrphanOutOnly);
        assert!(r.rows[0].needs_review);
        assert!(r.rows[0].first_check_in_at.is_none());
    }

    #[test]
    fn out_before_in_skew_flagged() {
        let visits = [visit(
            "IA",
            "3",
            "2026-08-08T14:00:00",
            None,
            "S1",
            "v1",
        )];
        let orphans = [CheckoutEvent {
            identity: id("IA", "3"),
            at: "2026-08-08T10:00:00".into(),
            station_id: "S2".into(),
            event_uid: "e1".into(),
        }];
        let r = rollup_master(&visits, &orphans);
        // Cannot pair out before in → still inside + needs_review
        assert_eq!(r.rows[0].status, MasterStatus::StillInside);
        assert!(r.rows[0].needs_review);
    }

    #[test]
    fn local_checkout_closed_row() {
        let visits = [visit(
            "IA",
            "4",
            "2026-08-08T09:00:00",
            Some("2026-08-08T11:00:00"),
            "S1",
            "v1",
        )];
        let r = rollup_master(&visits, &[]);
        assert_eq!(r.rows[0].status, MasterStatus::Left);
        assert_eq!(
            r.rows[0].last_check_out_at.as_deref(),
            Some("2026-08-08T11:00:00")
        );
        assert!(!r.rows[0].needs_review);
    }

    #[test]
    fn empty_inputs_yield_no_rows() {
        let r = rollup_master(&[], &[]);
        assert!(r.rows.is_empty());
        assert_eq!(unique_identity_count(&r.rows), 0);
    }

    #[test]
    fn two_different_identities_two_rows_sorted() {
        let visits = [
            visit("BR", "2", "2026-08-08T09:00:00", None, "S1", "a"),
            visit("IA", "1", "2026-08-08T08:00:00", None, "S1", "b"),
        ];
        let r = rollup_master(&visits, &[]);
        assert_eq!(r.rows.len(), 2);
        assert_eq!(r.rows[0].identity.category, "BR");
        assert_eq!(r.rows[1].identity.category, "IA");
        assert_eq!(unique_identity_count(&r.rows), 2);
    }

    #[test]
    fn reentry_two_closed_visits_left() {
        let visits = [
            visit(
                "IA",
                "5",
                "2026-08-08T09:00:00",
                Some("2026-08-08T10:00:00"),
                "S1",
                "v1",
            ),
            visit(
                "IA",
                "5",
                "2026-08-08T14:00:00",
                Some("2026-08-08T16:00:00"),
                "S1",
                "v2",
            ),
        ];
        let r = rollup_master(&visits, &[]);
        assert_eq!(r.rows.len(), 1);
        assert_eq!(r.rows[0].visit_count, 2);
        assert_eq!(
            r.rows[0].first_check_in_at.as_deref(),
            Some("2026-08-08T09:00:00")
        );
        assert_eq!(
            r.rows[0].last_check_out_at.as_deref(),
            Some("2026-08-08T16:00:00")
        );
        assert_eq!(r.rows[0].status, MasterStatus::Left);
    }

    #[test]
    fn two_orphans_pair_two_opens() {
        let visits = [
            visit("IA", "6", "2026-08-08T09:00:00", None, "S1", "v1"),
            visit("IA", "6", "2026-08-08T11:00:00", None, "S2", "v2"),
        ];
        let orphans = [
            CheckoutEvent {
                identity: id("IA", "6"),
                at: "2026-08-08T10:00:00".into(),
                station_id: "SX".into(),
                event_uid: "e1".into(),
            },
            CheckoutEvent {
                identity: id("IA", "6"),
                at: "2026-08-08T12:00:00".into(),
                station_id: "SY".into(),
                event_uid: "e2".into(),
            },
        ];
        let r = rollup_master(&visits, &orphans);
        assert_eq!(r.rows[0].status, MasterStatus::Left);
        assert!(r.rows[0].open_stations.is_empty());
        assert_eq!(
            r.rows[0].last_check_out_at.as_deref(),
            Some("2026-08-08T12:00:00")
        );
    }

    #[test]
    fn check_in_only_status() {
        let visits = [visit(
            "IA",
            "7",
            "2026-08-08T09:00:00",
            None,
            "S1",
            "v1",
        )];
        let r = rollup_visits_only(&visits);
        assert_eq!(r.rows[0].status, MasterStatus::StillInside);
        // single open station → StillInside; needs_review may be false for single open
        assert_eq!(r.rows[0].open_stations, vec!["S1".to_string()]);
    }

    #[test]
    fn stations_seen_includes_orphan_only_station() {
        let orphans = [CheckoutEvent {
            identity: id("X", "1"),
            at: "2026-08-08T12:00:00".into(),
            station_id: "EXIT".into(),
            event_uid: "e".into(),
        }];
        let r = rollup_master(&[], &orphans);
        assert!(r.rows[0].stations_seen.iter().any(|s| s == "EXIT"));
    }
}
