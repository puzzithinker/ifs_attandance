# IFS Attendance — documentation index

This folder explains how the system works for **operators**, **admins**, and **developers**.

| Doc | Audience | Contents |
|-----|----------|----------|
| [USER_GUIDE.md](./USER_GUIDE.md) | Desk operators, event admin | Day-of-seminar steps, modes, messages, multi-laptop merge |
| [MULTI_STATION.md](./MULTI_STATION.md) | Admin / planners | Cross-point check-in/out, soft check-out, master pairing, edge cases |
| [ARCHITECTURE.md](./ARCHITECTURE.md) | Developers | Crates, data model, flows, APIs, schema, packaging |
| [CUTOVER.md](./CUTOVER.md) | Tech lead | Deploy, backup, first-event checklist |
| [REFACTORING.md](./REFACTORING.md) | Developers | Suggested next refactors and why |
| [rust-rewrite-design.md](./rust-rewrite-design.md) | Architects | Original design decisions (K1–K25, PR plan) |

**Ship binary:** single exe `ifs_attendance` / `ifs_attendance.exe` — see root [README.md](../README.md).
