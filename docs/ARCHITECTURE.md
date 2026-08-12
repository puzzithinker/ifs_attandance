# Architecture — IFS Attendance (Rust)

## High-level

```text
┌─────────────────────────────────────────────────────────┐
│  ifs_attendance.exe  (ifs-app)                           │
│  eframe/egui · menus · mode pills · scan field · theme   │
└────────────┬───────────────────────────────┬────────────┘
             │                               │
             v                               v
      ┌─────────────┐                 ┌──────────────┐
      │  ifs-core   │ pure domain     │ ifs-storage  │
      │  parse      │◄────────────────│ rusqlite     │
      │  decide_scan│  types/cmds     │ migrate      │
      │  messages   │                 │ apply_mode   │
      │  rollup     │                 │ CSV/packages │
      └─────────────┘                 └──────┬───────┘
                                             │
                                             v
                                      agent.db / master.db
                                      station.toml
```

**Rule:** business rules live in `ifs-core` (no I/O). Persistence and files live in `ifs-storage`. GUI only wires them.

---

## Crates

### `ifs-core`

| Module | Responsibility |
|--------|----------------|
| `parse` | `parse_qr_url` — extract `categoryCode` + `licenseNo` |
| `service` | `decide_scan` — pure state machine |
| `messages` | `message_zh` / `outcome_from_parse_error` |
| `model` | DTOs: identity, mode, presence, outcomes, commands, rollup rows |
| `rollup` | `rollup_master` — multi-station pairing |

**No** filesystem, **no** SQLite, **no** egui.

### `ifs-storage`

| Module | Responsibility |
|--------|----------------|
| `db` | `open_database`, `open_database_with_station`, `open_in_memory` |
| `migrate` | `PRAGMA user_version` 0→2, legacy `attendance` import |
| `repository` | `SqliteVisitRepository::apply_mode`, counts, list snapshots |
| `store` | `AttendanceStore` — owns the `Connection`; `handle_scan`, counts, rollup, exports, packages, meta |
| `export` | UTF-8-BOM CSV, English month filenames, master CSV |
| `package` | Station package export/import (idempotent by uid) |
| `station` | `station.toml` load/create |
| `timeutil` | Local ISO timestamps |

### `ifs-app`

| Module | Responsibility |
|--------|----------------|
| `main` | CLI (`clap`), `--smoke` / `--master` / `--db` |
| `app` | App state, actions, `update` orchestration |
| `ui/*` | Panels: `top_bar`, `mode_selector` (pills + scan card), `metrics`, `status_banner`, `recent_list`, `master_dashboard`, `settings`, `open_db` (open/switch database + role at runtime) |
| `feedback` | Pure outcome → tone/headline/detail mapping (unit-tested) |
| `theme` | Colors, cards, status tones |
| `fonts` | System CJK load strategy |
| `paths` | Resolve default DB path |

`ifs-app` has **no `rusqlite` dependency** — all persistence goes through `AttendanceStore`.

Binary name: **`ifs_attendance`** (single exe).

---

## Domain state machine (desk)

Presence = open visit for identity on **this** DB (`check_out_at IS NULL`).

| Mode | Presence | soft_checkout | Outcome | Persist |
|------|----------|---------------|---------|---------|
| CheckIn | Outside | * | CheckedIn | InsertCheckIn |
| CheckIn | Inside | * | AlreadyCheckedIn | — |
| CheckOut | Inside | * | CheckedOut | UpdateCheckOut |
| CheckOut | Outside | false | NotCheckedIn | — |
| CheckOut | Outside | true | OrphanCheckOut | InsertOrphanCheckOut |

Parse failures never call `decide_scan`; UI maps them via `outcome_from_parse_error`.

---

## Schema (`user_version = 2`)

### `visits`

| Column | Notes |
|--------|-------|
| id | INTEGER PK |
| category, license_no | Identity |
| check_in_at, check_out_at | Local ISO `YYYY-MM-DDTHH:MM:SS` |
| created_at | Bookkeeping |
| station_id, visit_uid | Provenance; visit_uid UNIQUE |
| source_kind | `local` \| `imported` |
| notes | Reserved |

**Desk:** unique partial index  
`idx_visits_one_open ON (category, license_no) WHERE check_out_at IS NULL`

**Master:** that index is **dropped** so multi-station open rows can coexist. Verify schema does **not** require it on reopen.

### `scan_events`

Append-only audit + soft leaves (`OrphanCheckOut`).

### `app_meta` / `import_audit`

Role/station stamps and import history.

### Legacy migration

Python `attendance(保險中介人類別, 保險中介人編號, timestamp)` → visits; empty license skipped; empty category kept; table renamed `attendance_legacy_*`.

---

## Key control flows

### Scan (desk)

```text
Enter on scan field (lost_focus && Enter; auto-refocus when idle)
  → AttendanceStore::handle_scan(raw, mode, now)
       parse_qr_url ──err──► outcome_from_parse_error (no DB write)
       apply_mode(mode, identity, now)
            BEGIN
            load open visit
            decide_scan(...)
            apply PersistCommand + scan_events
            COMMIT
  → feedback.rs tone/headline/detail + counts refresh
```

### Master merge

```text
export_station_package(desk.db → package.db)
import_station_package(master, package)  -- INSERT OR IGNORE by visit_uid/event_uid
rollup_master(visits, orphan_events)
export_master_csv(...)
```

---

## Public APIs (frozen names)

```rust
// ifs-core
parse_qr_url(input) -> Result<AgentIdentity, ParseError>
decide_scan(mode, presence, identity, now, open_id, open_at, soft) -> (ScanOutcome, Option<PersistCommand>)
message_zh(&ScanOutcome) -> String
rollup_master(&[VisitSnapshot], &[CheckoutEvent]) -> PairingResult

// ifs-storage
open_database_with_station(path, DbRole) -> (Connection, MigrateReport, StationInfo)
AttendanceStore::open / handle_scan / counts / master_rollup_rows / export_* / import_package
SqliteVisitRepository::apply_mode / counts / export_csv / list_*
export_station_package / import_station_package
default_export_filename / export_master_csv
```

---

## Single-exe packaging

| Concern | Approach |
|---------|----------|
| SQLite | `rusqlite` feature `bundled` (static) |
| Release | LTO, strip, codegen-units=1 |
| Windows GUI | `windows_subsystem = "windows"` in release |
| Ship | Copy `ifs_attendance.exe` only |

Build: `cargo build --release -p ifs-app`  
Helper: `scripts/build-release.sh`

---

## Testing layers

| Layer | Where | What |
|-------|-------|------|
| Domain unit | `ifs-core` `#[cfg(test)]` | Parse, decide, messages, rollup |
| Storage unit | `ifs-storage` modules | Filename, station.toml |
| Storage integration | `tests/storage_integration.rs` | Real rusqlite paths |
| Crash/resume | `tests/storage_integration.rs` | Drop store → reopen same file: visits, orphans, meta, import idempotency |
| App | `app`/`feedback`/`paths` unit tests; `--smoke` CLI | Scan vs system-message routing, store switching; no display required |

---

## Non-goals (current product)

- Live multi-writer / central server during seminar  
- Cloud sync of open DBs  
- Camera QR decoding  
- Multi-event `event_id` productization  
- Auth / multi-operator accounts  

See design doc Key Decisions K1–K25 for rationale.
