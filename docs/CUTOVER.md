# Deploy & first-event checklist

The Python Tk app (`ifs_app.py` / `ifs_app.exe`) is **no longer in this repository**. Ship only `ifs_attendance.exe`.

## Before any real seminar

1. Build release: `cargo build --release -p ifs-app`  
2. Copy `target/release/ifs_attendance.exe` to a USB stick  
3. On a pilot laptop:
   - Run the exe; confirm UI + Chinese labels  
   - Scan a test QR (入場 / 離場)  
   - Export desk CSV; open in Excel  
4. If upgrading an **old Python `agent.db`**:
   - Copy → `agent.db.pre-rust-YYYYMMDD.bak` **before** first open  
   - Open with Rust; smoke/About should show migration  
   - Confirm row counts look right  

## Multi-laptop dry run (once)

1. Two PCs: check-in on A, check-out on B (soft check-out)  
2. Export station packages → admin `--master` import → master CSV  
3. Confirm one row for the test agent with first_in / last_out  

## Live desks

1. Each desk: own folder + exe + unique `station_name` in `station.toml`  
2. Entry desks: **入場**; exit desks: **離場**  
3. Do not share a live `agent.db` over OneDrive/Syncthing  

## After the event

1. Every desk: **匯出站點包** → USB  
2. Admin: `ifs_attendance.exe --master --db master.db` → import all → master CSV  
3. Archive packages + `master.db` + CSV (license numbers are sensitive)  

## Never

- Commit `agent.db` / `station.toml` to git  
- Hot-swap one open DB between machines while scanning  
- Delete the only pre-migration backup until the seminar archive is confirmed  

## Old Python DBs only

If a file still has table `attendance` (Python era), Rust migrates to `visits` and renames the old table to `attendance_legacy_*`. Keep a file backup; there is no Python app left in-repo to fall back to.
