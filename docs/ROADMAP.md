# Roadmap

Phases are ordered so each one gives something usable and tests the riskiest
assumption early. Status as of 2026-11-27.

| # | Phase | Delivers | Status |
|---|---|---|---|
| 0 | Foundations | Interview, decision log, research, architecture | done |
| 1 | Core | Rust domain model, schedule engine (all kinds), SQLite storage, operation log, CLI, tests | done (merged 2026-11-27, PR #1); see below |
| 2 | App shell | Tauri 2 + Svelte on Android and web; items, tasks, one-tap completion; **prove local notifications with actions on Android** | built on branch `phase-2-app-shell`; web verified in a browser; Android APK built; **on-device reminder proof not yet run** (see below) |
| 3 | Onboarding + templates | 10-question questionnaire, generic template set, prioritisation (life safety first), start-small activation | |
| 4 | Records | Photos and documents, custom fields, home facts sheet, QR labels, costs | |
| 5 | House binder v1 | Typst binder: emergency page, main volume, sealed volume, change tracking for reprints | |
| 6 | Sync + encryption | Self-hosted server, end-to-end encryption, multi-device, members and roles, recovery key | |
| 7 | Home Assistant | Tasks as HA to-do/calendar/sensors, HA meters and events trigger tasks, device import, server as add-on | |
| 8 | Floor plans | Stage A: rooms on a grid, floors, item pins. Stage B: measured walls, angles, windows, area, PDF | |
| 9 | Contractors and alerts | Contractor records and job history, warranty expiry, CPSC recall checks, insurance export, binder editions (handover, succession) | |
| 10 | iOS | Apple account, Mac/cloud builds, App Store | |
| 11 | Hosted + extras | Paid hosting, opt-in disclosure extras (email/SMS reminders, recalls), AI label reading, share links for contractors and tenants | |
| 12 | Landlord depth | Multi-property dashboards, tenant problem reports, per-property costs | |

Label scanning on the device (D-29) lands with Phase 4 on Android and the web.
Opt-in research data (D-32) lands with Phase 6, when there is a server to send to.

## Phase 1 notes

Built: the `housewright-core` crate (domain model with sensitivity flags,
pure schedule engine for every D-20 kind, prioritisation with start-small and
welcome-back, SQLite storage with migrations, operation log with hybrid logical
clocks and convergent merge, audit view, JSON export/import documented in
[FILE-FORMAT.md](FILE-FORMAT.md), TOML template format with ten generic starter
templates), and the `hw` command-line tool.

Not in Phase 1, by design: networking, encryption, file storage for
attachments (hash references only), contractor and fact commands in the CLI.
Design choices made along the way are recorded as *Proposed* D-50 to D-58.


## Phase 2 notes

Built: `housewright-api`, one JSON command surface over the core, shared by
every front end (D-63). `housewright-web` compiles that API to WebAssembly,
with SQLite on OPFS in a worker and one tab owning the database (D-61). The
Svelte 5 interface (`app/`) has these screens: Today (ranked, one-tap Done
with Undo and an optional note afterwards, reschedule to a moment, review
after three snoozes, welcome back after a lapse), Things grouped by room or
kind, thing detail, Add a thing from a bundled template with start small,
History, a three-question first run, and Settings (quiet hours, maintenance
window, export). The Tauri 2 shell and a custom reminders plugin cover
Android (D-62). The core gained a pure reminder planner and per-member
reminder times (D-59) and a lapse check (D-60).

Verified: Rust format, clippy (native and wasm) and tests; svelte-check;
vitest; and a Playwright run of the production web build in Chrome. That run
covers first run, add from template, start small, Done, History, a second tab
being refused, and data surviving a reload. The x86_64 debug APK builds.

**Not yet verified: the Android reminder proof.** The build agent may not run
`adb` on this machine, so the four checks (fires in the background, fires
after `am kill`, Done from the notification recorded in History, survives a
reboot) have not been run. `scripts/reminder-proof.sh` runs them step by step
for the owner and saves the evidence in `docs/phase-2-evidence/`. Phase 3
should not build on reminders until that passes.

Not built in Phase 2: photos ("Done + photo" waits for attachments in
Phase 4), the weekly digest (P5), adding a calendar entry when rescheduling
(P6), reminders on the web when the page is closed (Phase 11), iOS
(Phase 10), and the full questionnaire (Phase 3).
