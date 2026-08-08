# Test fixtures

Most fixtures are built **in-process** by tests (no fragile binary blobs in git):

| Test | Fixture built at runtime |
|------|---------------------------|
| `legacy_attendance_migration` | Python-shaped `attendance` table |
| `soft_checkout_orphan_and_cross_station_pair` | Two desk DBs + master |
| `master_reopen_after_drop_open_index_and_import` | Master without open-unique index |
| CSV tests | Temp files with BOM |

## Running tests

```bash
cargo test --workspace
cargo test -p ifs-core
cargo test -p ifs-storage
cargo test -p ifs-app
```

## Optional manual golden

Create a Python-era DB with `attendance` rows and open it with `ifs_attendance --smoke --db …` to verify migration on real hardware.
