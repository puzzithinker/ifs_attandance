# Further refactoring opportunities

This is a **backlog of improvements**, not unfinished work. The system is usable as-is. Items are ordered roughly by value vs risk.

---

## High value / low risk

### 1. Split `SqliteVisitRepository` vs connection ownership

**Today:** GUI holds `rusqlite::Connection` and builds a short-lived repository each call.

**Refactor:** `struct AttendanceStore { conn, station, role, soft }` owned by the app; methods `apply`, `counts`, `export`. Removes `rusqlite` from `ifs-app`’s direct dependency surface.

### 2. Single orchestration function for “scan string → outcome”

**Today:** GUI parses, then `apply_mode`, then messages.

**Refactor in core or a thin `ifs-service`:**

```rust
fn handle_scan(input, mode, soft, clock, repo) -> ScanOutcome
```

Makes GUI and CLI share one path; easier testing of invalid QR → no DB write.

### 3. Clock injection everywhere

**Today:** `now_iso` passed into `apply_mode`; good for domain. Package timestamps use `now_iso_local()` internally.

**Refactor:** pass `impl Fn() -> String` or `trait Clock` into storage for fully deterministic package/export tests.

### 4. Extract UI widgets

**Today:** mode pills / metric cards / status banner live in `app.rs`.

**Refactor:** `ui/mode_pills.rs`, `ui/metrics.rs`, `ui/status_banner.rs` for readability and future snapshot tests.

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
| GUI: extract pure “view model” | Test status tone without eframe |

---

## What not to refactor casually

- Identity = `(category, license_no)` — product invariant  
- Soft check-out default on — multi-door seminars depend on it  
- Master dropping `idx_visits_one_open` — required for multi open  
- Frozen API names (`parse_qr_url`, `decide_scan`, `message_zh`, `apply_mode`) without a versioned plan  

---

## Suggested order if you continue engineering

1. `handle_scan` orchestration + more integration tests (this pass already deepens tests)  
2. `AttendanceStore` ownership cleanup  
3. UI module split  
4. Only then event-sourcing or live LAN  

Keep shipping the single exe; refactors should not force multi-file installs.
