# IFS Attendance (Rust) — single exe

Windows-oriented kiosk for IFS AML seminar **check-in / check-out** (Rust single-exe).

## Documentation (start here)

| Doc | Purpose |
|-----|---------|
| **[docs/README.md](docs/README.md)** | Index of all docs |
| **[docs/USER_GUIDE.md](docs/USER_GUIDE.md)** | Operators & admin day-of guide |
| **[docs/MULTI_STATION.md](docs/MULTI_STATION.md)** | Check-in at A, check-out at B (soft check-out + master merge) |
| **[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)** | Crates, schema, APIs, packaging |
| **[docs/CUTOVER.md](docs/CUTOVER.md)** | Deploy / backup / first-event checklist |
| **[docs/REFACTORING.md](docs/REFACTORING.md)** | Optional next engineering cleanups |
| **[docs/rust-rewrite-design.md](docs/rust-rewrite-design.md)** | Full design decisions (K1–K25) |

## Single executable

One file. No Python. SQLite **inside** the binary.

```bash
cargo build --release -p ifs-app
# Windows → target/release/ifs_attendance.exe
# Linux   → target/release/ifs_attendance

./scripts/build-release.sh   # copies into dist/
```

Copy only the exe to each seminar laptop. First run creates `agent.db` + `station.toml` beside it.

```bash
ifs_attendance.exe
ifs_attendance.exe --db D:\event\agent.db
ifs_attendance.exe --master --db master.db
ifs_attendance.exe --smoke --db agent.db
```

## Develop & test

```bash
cargo test --workspace
cargo run -p ifs-app -- --smoke --db ./agent.db
cargo run -p ifs-app -- --db ./agent.db
```

## Workspace

| Crate | Role |
|-------|------|
| `ifs-core` | Pure domain: parse, decide, messages, rollup |
| `ifs-storage` | SQLite, migrate, CSV, packages |
| `ifs-app` | eframe GUI → binary **`ifs_attendance`** |

## Cross-door seminars (summary)

Agents may **enter at A** and **leave at B**. Exit desks use **離場** with **soft check-out** (default on). End of day: export station packages → admin `--master` import → master CSV pairs times. Details: [docs/MULTI_STATION.md](docs/MULTI_STATION.md).

## Legacy DBs

The old Python Tk app has been **removed** from this repo. If you still have an old `agent.db` with an `attendance` table, the Rust app migrates it automatically on first open — **file-backup first**. See [docs/CUTOVER.md](docs/CUTOVER.md).
