# Further refactoring opportunities

This is a **backlog of improvements**, not unfinished work. The system is usable as-is. Items are ordered roughly by value vs risk.

---

## High value / low risk

### 1. Split `SqliteVisitRepository` vs connection ownership — ✅ Done

`AttendanceStore` (`ifs-storage::store`) owns the `Connection` + `StationInfo` + `DbRole` + `soft_checkout` and exposes `handle_scan`, `counts`, `master_rollup_rows`, CSV/package export/import, event name, sound flag, `rename_station`. `ifs-app` no longer depends on `rusqlite` (dropped from its `Cargo.toml`; `bundled` still comes via `ifs-storage` feature unification, so the single-exe static SQLite guarantee is unchanged).

### 2. Single orchestration function for “scan string → outcome” — ✅ Done

`AttendanceStore::handle_scan(raw, mode, at) -> ScanResult` — parse → apply → outcome, never returns `Err` (parse failures → `InvalidQr`/`EmptyInput`, DB failures → `Failed`), with `identity: Option<AgentIdentity>` so the UI can show the subject even on DB failure. Covered by storage unit tests (duplicate check-in writes nothing, garbage QR writes nothing, etc.). The timestamp is caller-supplied (clock injection at the call site).

### 3. Clock injection everywhere

**Today:** `now_iso` passed into `handle_scan`/`apply_mode`; good for domain. Package timestamps use `now_iso_local()` internally.

**Refactor:** pass `impl Fn() -> String` or `trait Clock` into storage for fully deterministic package/export tests.

### 4. Extract UI widgets — ✅ Done

`ifs-app/src/ui/`: `top_bar`, `mode_selector` (pills + scan card), `metrics`, `status_banner` (session chips + last result), `recent_list`, `master_dashboard`, `settings` (floating `egui::Window`). Pure outcome→UI mapping lives in `ifs-app/src/feedback.rs` with unit tests for every `ScanOutcome` variant; `app.rs` (~480 lines) keeps only state, orchestration, and actions. Same pass also fixed: Enter now submits via `lost_focus() && Enter` only; scan field auto-reclaims focus when nothing else has it (kiosk scanner safety); system messages (export/關於/settings) no longer pollute the recent-scan list, session counters, or scan sounds.

---

## Medium value

### 5. Event sourcing lite for visits

**Today:** mutable `check_out_at` + separate `scan_events`.

**Refactor:** treat `scan_events` as source of truth; derive open visits for desk; simplifies audit. Larger migration.

### 6. Master pairing applied back into visit rows

**Today:** pairing is pure in rollup for CSV only; imported open visits stay open in SQL.

**Refactor:** optional “materialize pairing” step that sets `check_out_at` on master for reporting consistency in SQL queries.

### 7. Station config in DB, not only `station.toml`

**Today:** toml next to DB; package reads meta/station_id from visits.

**Refactor:** always stamp `app_meta` as canonical; generate toml as export of meta. One source of truth.

### 8. Error type unification

**Today:** `ParseError`, `StorageError`, stringy GUI failures.

**Refactor:** `thiserror` enum `AppError` with display layer for Chinese UI strings.

### 9. Reduce clap surface in GUI builds

**Today:** clap always linked.

**Refactor:** feature `cli` for smoke/master flags vs pure GUI binary if size matters.

---

## Larger product refactors

### 10. Optional LAN “live master” mode

For organizers who **have** reliable Wi‑Fi: desk posts events to a small local server; live venue 目前在場. Keep offline package path as fallback. Big scope; don’t mix into offline core without a feature flag.

### 11. Soft check-out UX copy / station presets

Presets: 入口 / 出口 names; banner always explains 跨站點. Small UI product work, not deep refactor.

### 12. Replace egui with Slint if Windows font/IME issues appear

Design already allows Slint fallback (K1b). Would rewrite presentation only if core/storage stay stable.

### 13. `async` / sqlx

**Not recommended** for this kiosk: single-threaded UI + sync rusqlite is appropriate.

---

## Test debt (refactor of test layout)

| Idea | Why |
|------|-----|
| `ifs-core` integration module tests file | Keep unit modules small |
| Golden files under `testdata/` for CSV bytes | Catch BOM/header regressions |
| Property tests for rollup (proptest) | Random in/out sequences |
| ~~GUI: extract pure “view model”~~ ✅ `feedback.rs` | Tone/headline/detail tested without eframe |

---

## What not to refactor casually

- Identity = `(category, license_no)` — product invariant  
- Soft check-out default on — multi-door seminars depend on it  
- Master dropping `idx_visits_one_open` — required for multi open  
- Frozen API names (`parse_qr_url`, `decide_scan`, `message_zh`, `apply_mode`) without a versioned plan  

---

## Suggested order if you continue engineering

1. ~~`handle_scan` orchestration~~ ✅, ~~`AttendanceStore` ownership~~ ✅, ~~UI module split~~ ✅  
2. Clock injection in storage internals (item 3)  
3. Error type unification (item 8)  
4. Only then event-sourcing or live LAN  

Keep shipping the single exe; refactors should not force multi-file installs.
