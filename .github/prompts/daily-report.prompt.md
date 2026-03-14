---
description: Sync all characters and produce a location-aware expiry report. Run this at the start of a PI session.
---

1. Call `sync_characters` to pull fresh data from ESI for all enrolled characters.
2. Call `get_expiring_programs` with `hours: 168` (7 days) to show the full week ahead.
3. Call `suggest_schedule` to produce a prioritised reset order.

Present the output in three sections:
- **Characters in the hole** (no travel needed) — their expiring programs, sorted by urgency.
- **Characters who need to travel** — list with their most urgent expiry so they know how quickly they need to move.
- **Reset schedule** — the full ranked list from `suggest_schedule`.

Use planet labels throughout (e.g. "Rebort Gas V"). Express expiry times as "in Xh Ym" relative to now.
