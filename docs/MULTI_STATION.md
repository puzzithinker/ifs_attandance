# Multi-station & cross-point check-out

## The real seminar problem

Agents use **entry point A**. They are **not** guaranteed to leave at A. They may leave at B, C, or a side door.

```text
        [入口 A]  check-in
            |
            v
        seminar hall
            |
     +------+------+
     v             v
 [出口 B]       [出口 C]   check-out (maybe)
```

If every laptop only writes **local SQLite** and there is **no reliable Wi‑Fi**, you cannot update a shared “who is inside” table live. This product solves that with:

1. **Local recording** on every desk (including soft leave at B without A’s data)  
2. **End-of-day package merge** on an admin laptop  
3. **Deterministic pairing** of check-ins and check-outs per agent identity  

---

## Design principles

| Principle | Choice |
|-----------|--------|
| Network during event | **Not required** |
| Live shared DB | **Forbidden** (corruption / conflict risk) |
| Same agent on many desks | **Allowed** |
| Check-out desk ≠ check-in desk | **Supported** via soft check-out + master pairing |
| Authoritative attendance list | **Master CSV** after import |
| Live “venue currently inside” | **Not** guaranteed offline (only per-desk 目前在場) |

---

## Soft check-out (desk behavior)

### Strict local only (if soft check-out were off)

| Mode | Local presence | Result |
|------|----------------|--------|
| 離場 | Outside | 尚未入場，無法離場 — **no write** |

Exit desk B would fail for anyone who entered on A.

### Soft check-out **on** (default, K24)

| Mode | Local presence | Result | Stored |
|------|----------------|--------|--------|
| 離場 | Outside | 已登記離場 (跨站點) | `scan_events` row `outcome=OrphanCheckOut` |
| 離場 | Inside | 已登記離場 | Visit row gets `check_out_at` |

CLI: `--soft-checkout` defaults to true.

---

## Master pairing algorithm

Identity key: `(category, license_no)`.

Inputs:

- All `visits` from all imported stations (each has `station_id`, `visit_uid`, `check_in_at`, optional `check_out_at`)  
- All orphan leave events (`OrphanCheckOut`)

Per identity:

1. **first_check_in_at** = minimum of all check-in times  
2. Visits that already have `check_out_at` count as locally closed  
3. **Unmatched opens** = visits with null check-out  
4. Sort orphan leave times; each leave pairs to the **earliest unmatched open** with `open_at ≤ leave_at`  
5. Leftover opens → still inside; leftover leaves / skew → **needs_review**

```text
Example:
  A: check-in  10:00  (open)
  B: orphan out 12:00
→ Paired: first_in=10:00, last_out=12:00, status=已離場
```

```text
Example:
  A: check-in 10:00 open
  C: check-in 10:30 open   (same person scanned entry twice?)
→ status=仍在場, needs_review=1, stations list both
```

```text
Example:
  B: orphan out 12:00 only
→ status=僅離場(無入場), needs_review=1
```

```text
Example:
  A: check-in 14:00
  B: orphan out 10:00  (clock wrong or leave before entry)
→ cannot pair; still inside + needs_review
```

---

## Operator checklist (printable)

### Setup (morning)

- [ ] Each laptop: unique folder + `ifs_attendance.exe`  
- [ ] Edit `station_name` (入口-1, 出口-2, …)  
- [ ] Entry desks: mode **入場**  
- [ ] Exit desks: mode **離場**  
- [ ] Test one real QR end-to-end  

### During event

- [ ] Entry: only 入場 scans  
- [ ] Exit: only 離場 scans (always scan leavers)  
- [ ] Do not share one live `agent.db` across machines  

### End of event

- [ ] Every desk: **匯出站點包** → USB `/Stations/`  
- [ ] Admin: `--master --db master.db`  
- [ ] Import all packages  
- [ ] Export master CSV  
- [ ] Spot-check `needs_review=1` rows  
- [ ] Archive packages + master.db + CSV by date  

---

## FAQ

**Q: Can we see global headcount live?**  
A: Not with offline multi-laptop. Only local 目前在場. For live global counts you would need a network service (explicit non-goal today).

**Q: Person checks in twice at two entries?**  
A: Master may show two open visits / 仍在場 + needs_review. Attendance still counts once as unique identity for “did they show up” style rollups (one master row).

**Q: Re-import same package?**  
A: Safe. Same `visit_uid` / `event_uid` → skipped.

**Q: Soft check-out pollutes exit desk counts?**  
A: Orphan leaves are **events**, not open visits, so they do not inflate 目前在場 on the exit desk.

---

## Related code (for developers)

| Piece | Location |
|-------|----------|
| Soft decide | `ifs-core` `decide_scan(..., soft_checkout: true)` |
| Persist orphan | `ifs-storage` `InsertOrphanCheckOut` → `scan_events` |
| Pairing | `ifs-core` `rollup_master` |
| Packages | `export_station_package` / `import_station_package` |
| Master UI | `ifs-app` `--master`, import/export menus |
| Tests | `soft_checkout_orphan_and_cross_station_pair`, rollup unit tests |
