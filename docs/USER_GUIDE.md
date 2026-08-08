# User guide — IFS AML Seminar Attendance

## What this app does

At an IFS AML seminar, insurance intermediaries (agents) scan a **中介人一戶通 QR Code**. The app records:

- **入場 (check-in)** — agent entered  
- **離場 (check-out)** — agent left  

Data is stored in a local file `agent.db` next to the program.  
You ship **one** program file (`ifs_attendance.exe` on Windows). No Python install.

---

## Concepts (short glossary)

| Term | Meaning |
|------|---------|
| **Identity** | Pair of 保險中介人類別 (`categoryCode`) + 保險中介人編號 (`licenseNo`) from the QR URL |
| **Visit** | One stay: check-in time + optional check-out time |
| **Open visit** | Checked in, not yet checked out on **this** laptop |
| **Desk** | Normal mode: scan and record on this machine |
| **Master** | Admin mode: merge packages from all desks after the event |
| **Station** | This laptop’s name/id (`station.toml`) so merges know which desk wrote each row |
| **Soft check-out** | Leave scan recorded even if this laptop never saw the check-in (other exit desk) |
| **Station package** | Export of this laptop’s DB for the admin to import |

---

## First launch on a laptop

1. Copy `ifs_attendance.exe` to a folder (e.g. `D:\IFS_Attendance\`).
2. Double-click the exe.
3. The app creates:
   - `agent.db` — database  
   - `station.toml` — station id + name (default name `desk-1`)
4. Optional: edit `station.toml` and set a clear name:

```toml
station_id = "…do-not-change…"
station_name = "入口-1"
```

Keep `station_id` stable for the life of that laptop’s data. Rename only `station_name` for display (入口 / 出口 / 側門).

---

## Desk day — single laptop

### Screen layout

- **Header** — Desk vs Master, File/Help menus  
- **Mode pills** — large **入場** (green) / **離場** (orange)  
- **QR field** — always ready for scanner (keyboard wedge + Enter)  
- **目前在場** / **累計人次** — live counts on **this** machine only  
- **狀態 banner** — last result (green / amber / red / blue)

### Workflow

1. Choose mode:
   - **入場** for entry queue  
   - **離場** for exit queue  
2. Focus stays on the scan box.  
3. Scanner pastes the QR URL and sends **Enter**.  
4. Field clears automatically. Status updates.

### Status messages (Chinese)

| Banner text | Meaning | Tone |
|-------------|---------|------|
| 已登記入場 | New check-in stored | Green |
| 已在場內 (重複入場) | Already open visit on **this** desk | Amber |
| 已登記離場 | Check-out closed an open visit on **this** desk | Green |
| 已登記離場 (跨站點) | Soft check-out: leave recorded without local check-in | Blue |
| 尚未入場，無法離場 | Soft check-out **off** and no local open visit | Amber |
| QR Code 無效 | Missing/empty `categoryCode` or `licenseNo` | Red |
| 請掃描 QR Code | Empty scan | Gray |
| 操作失敗: … | Storage/system error | Red |

### Export CSV (this desk only)

**檔案 → 匯出出席 CSV…**

- UTF-8 with BOM (Excel-friendly)  
- Columns include category, license, 入場時間, 離場時間, station_id, visit_uid  
- Default name like `IFS_AML_seminar_attendance_07-August.csv` (English month names)

This export is **one laptop’s data**, not the whole venue.

---

## Multiple laptops (typical seminar)

Agents often **check in at A** and **check out at B** (or C). That is supported.

### During the event

| Role | Mode | Action |
|------|------|--------|
| Entry desks | 入場 | Scan every entrant |
| Exit desks | 離場 | Scan every leaver (**even if they entered on another laptop**) |
| Mixed desk | Switch pills as needed | Soft check-out stays enabled by default |

**Important**

- Each laptop’s **目前在場** is only local.  
- Whole-venue truth is built **after** the event on the admin machine.

### End of day — master merge

```text
Laptop A  → Export station package ─┐
Laptop B  → Export station package ─┼→ USB → Admin master DB → Master CSV
Laptop C  → Export station package ─┘
```

**On each desk**

1. **檔案 → 匯出站點包…**  
2. Save e.g. `IFS_station_入口-1_20260808-180000.db` to USB.

**On admin laptop**

```text
ifs_attendance.exe --master --db master.db
```

1. **檔案 → 匯入站點包…** — select all packages (safe to re-import; duplicates ignored).  
2. Review counts (總出席 / 累計人次).  
3. **檔案 → 匯出主控 CSV…** — one row per agent for compliance.

Master CSV fields (conceptual):

- 保險中介人類別 / 保險中介人編號  
- first_check_in_at  
- last_check_out_at (empty if still “open” after pairing)  
- stations_seen  
- visit_count  
- status: 已離場 / 仍在場 / 僅入場 / 僅離場(無入場)  
- needs_review: `1` if staff should glance at the row  

Details: [MULTI_STATION.md](./MULTI_STATION.md).

---

## CLI flags

```text
ifs_attendance.exe [OPTIONS]

  --db <path>           Database file (default: agent.db next to exe)
  --master              Master merge role
  --smoke               No GUI; print station/counts and exit
  --soft-checkout       Default true; record cross-desk leave
```

Examples:

```text
ifs_attendance.exe
ifs_attendance.exe --db D:\event\day1.db
ifs_attendance.exe --master --db master.db
ifs_attendance.exe --smoke --db agent.db
```

---

## Rules operators should know

1. **Same license, different category** are **different** people.  
2. Bad QR (empty category/license) is **rejected**, never stored.  
3. Re-entry after check-out creates a **new** visit row.  
4. Do **not** copy a live `agent.db` while the app is open; use **Export station package**.  
5. Set PC **timezone** correctly; times are local wall clock.  
6. Backup `agent.db` before major changes or before first open of an old Python-era DB.

---

## Troubleshooting

| Symptom | What to try |
|---------|-------------|
| Chinese looks like tofu (□□) | Windows should load YaHei; install CJK fonts or run on a normal Win10/11 image |
| Scanner types but no submit | Ensure Enter is sent; click the scan field once |
| 重複入場 but they just arrived | They may already be open on **this** desk; check 離場 first or master later |
| Exit desk shows 跨站點 | Expected when entry was on another laptop — good |
| Master import does nothing new | Re-import is idempotent; row already present |
| Counts “wrong” vs other desk | Local counts only; use master CSV for venue totals |

---

## Privacy note

The DB holds intermediary category and license numbers and timestamps. Treat USB packages and `master.db` as **sensitive event data**; archive under controlled access after the seminar.
