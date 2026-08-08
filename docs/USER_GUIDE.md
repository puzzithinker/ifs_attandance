# User guide — IFS Event Attendance（活動出席）

**完整繁體中文操作手冊（含多站點）：[USER_GUIDE_zh-Hant.md](./USER_GUIDE_zh-Hant.md)**

## What this app does

At any IFS event (seminar, training, briefing, …), participants scan a **中介人一戶通 QR Code**. The app records:

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

- **Header** — live **clock**, activity/event name, menus  
- **檢視** menu — 全螢幕 (F11), 音效開關, **活動/站點設定**  
- **Mode pills** — **入場** (green) / **離場** (orange)  
- **QR field** — scanner + Enter  
- **Metrics** — 目前在場 / 累計人次 / 本機成功 (desk); 總出席 / 需覆核 / 仍在場 (master)  
- **最近一次掃描** — outcome + identity + **N 秒前**  
- **本機近期掃描** — last ~12 rows with **複製** (copies 類別·編號)  
- **主控儀表板** (master only) — needs_review count + still-inside / review previews from rollup  

### Settings (檢視 → 活動 / 站點設定)

| Field | Stored in | Effect |
|-------|-----------|--------|
| 活動名稱 | `app_meta.event_name` | CSV `event` column + export filename slug |
| 站點顯示名 | `station.toml` | Shown in UI; station packages |

Restart reloads both for the same DB folder.

### Workflow

1. Set **活動名稱** / **站點** once if needed.  
2. Choose **入場** or **離場**.  
3. Scan QR (Enter). Field clears; result card + history update; optional **beep**.  
4. Use **複製** on recent rows to copy identity to clipboard.

### Last scan result (what the big card means)

| Headline | Meaning | Tone |
|----------|---------|------|
| 入場成功 | New check-in stored | Green |
| 重複入場 | Already open on **this** desk; not written again | Amber |
| 離場成功 | Closed open visit on **this** desk | Green |
| 跨站點離場 | Soft leave recorded (entry may be on another laptop) | Blue |
| 無法離場 | Soft check-out off and no local open visit | Amber |
| QR 無效 | Missing/empty category or license | Red |
| 操作失敗 | Storage/system error | Red |

The card also shows **category · license**, explanation, wall time, and **how long ago** the scan was.

### Keyboard / kiosk

| Key / control | Action |
|---------------|--------|
| **F11** or 檢視 → 全螢幕 | Toggle fullscreen |
| 檢視 → 音效 | Toggle success/fail beep (default **on**; stored in DB) |

### Export CSV (this desk only)

**檔案 → 匯出出席 CSV…**

- UTF-8 with BOM (Excel-friendly)  
- Columns: **event**, ID, category, license, 入場/離場 times, station_id, visit_uid  
- Filename includes event slug when set, e.g. `IFS_attendance_CPD_07-August.csv`
- Default name like `IFS_attendance_07-August.csv` (English month names)

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
