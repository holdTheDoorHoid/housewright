# Architecture

Status: proposed, 2026-11-27. Decisions referenced as D-nn are in
[DECISIONS.md](DECISIONS.md).

## Shape

```
              ┌──────────────── one Svelte interface ────────────────┐
              │                                                      │
   Android / iOS / desktop (Tauri 2)                     Web page (browser)
              │                                                      │
     housewright-core (native)                     housewright-core (WebAssembly)
     SQLite on disk                                SQLite-wasm on OPFS
              │                                                      │
              └──────── optional: encrypted operations ──────────────┘
                                     │
                     housewright-server (Rust; self-hosted,
                     Home Assistant add-on, or paid hosted)
                     stores and relays ciphertext only
```

Everything that matters lives in `housewright-core`. The apps are thin; the
server is dumb. That is what lets the app work alone on one phone (D-11) and
lets the server stay unable to read anything (D-14).

## Crates and packages

| Path | What it is |
|---|---|
| `crates/core` | Domain model, schedule engine, prioritisation, storage, operation log. No platform code. |
| `crates/crypto` | Household keys, key wrapping, operation and file encryption (D-15). |
| `crates/sync` | Operation exchange with a server; hybrid logical clocks; merge (D-13). |
| `crates/binder` | House binder and reports through Typst (D-43). |
| `crates/server` | Sync relay, share links, paid extras. |
| `crates/cli` | A command-line front end over the core, for testing and for power users. |
| `app/` | Svelte interface and Tauri 2 shell (Android, iOS, desktop). |
| `web/` | Web build: the same interface with the core compiled to WebAssembly. |
| `integrations/home-assistant` | Custom integration and add-on (D-26). |
| `templates/` | Community template library (D-21; may split into its own repo, Q-05). |

## Data model (core entities)

- **Household**: the sharing and encryption boundary. Has members and roles (D-23).
- **Property**: a house. A household can hold several (landlords).
- **Floor → Room**: places within a property. Floors carry the floor plan geometry.
- **Item**: something in the house (water heater, smoke alarm, roof). Optional
  make, model, serial, purchase and install dates, warranty, location (room +
  plan position), parent item (a filter inside a furnace), custom field values,
  attachments. Linked to the template it came from.
- **Template**: a reusable description of an item kind ("washing machine") or a
  specific model, with field definitions and recommended tasks. Templates
  inherit: model → generic → category.
- **Task**: recurring work on an item or a place. Has a schedule (D-20), an
  owner (P8), priority class, steps, why-it-matters text with its source,
  completion field definitions, and a state (scheduled, due soon, due, overdue,
  snoozed, paused, not applicable).
- **Completion**: append-only record of a task done: who, when, notes, photos,
  costs, custom field values, contractor. Never edited, only annotated or
  voided by a later entry.
- **Meter**: a usage counter (runtime hours, loads), updated by hand or by
  Home Assistant, used by usage-based schedules.
- **Contractor**: contact, trades, licence and insurance expiry, private rating;
  linked to completions for job history and cost (D-28).
- **Attachment**: an encrypted file (manual, receipt, warranty, photo) referenced
  by content hash.
- **Fact**: home facts sheet entries (D-27), each marked normal or sensitive for
  the binder (D-41).
- **Audit entry**: every operation is already attributed and timestamped in the
  operation log; the audit view is a readable projection of it.

## Schedule engine

A pure function: given a task's schedule, its completion history, meter
readings, the property's location and climate data, and "now", return the next
due window and the current state. Pure means it is easy to test exhaustively and
gives identical answers on every device. Kinds:

- **Fixed**: calendar recurrence (every N days/weeks/months/years, nth weekday,
  month-of-year), independent of completion.
- **Floating**: N units after the last completion.
- **Seasonal**: a named window ("before first frost", "spring") resolved against
  the property's climate data and hemisphere, with a fallback month range.
- **Usage**: due when a meter passes a threshold since the last completion,
  optionally with a calendar cap ("every 300 hours or 6 months, whichever first").
- **One-off**: a single due date.

Due dates are *windows* (earliest, due, overdue-after), not instants, because
"sometime in October" is how home maintenance really works.

## Sync and encryption (summary)

Every change becomes an operation `{id, hlc, author, entity, field, value}`.
Operations are applied to SQLite locally, encrypted with the household key, and
sent to the server when sync is on. Merge rules: last-writer-wins per field by
hybrid logical clock; append-only sets for completions, attachments and notes;
tombstones for deletes. Because merging is deterministic and per-field, the user
is never asked to resolve a conflict (anti-pattern: "an app that itself needs
maintenance").

## Reminders

Each device computes upcoming due windows and schedules local notifications
(D-18), respecting the person's quiet hours and maintenance window, the cap on
repeats, and the weekly digest (P5). Notification actions: Done, Done + photo,
Reschedule (P1, P6, P7).

## Things that need care

- **Background reminders in Tauri on mobile** are the least mature part of the
  stack. Phase 2 must prove scheduled local notifications with action buttons on
  Android before building on them.
- **SQLite in the browser** needs OPFS and a single-tab lock; the web build must
  handle a second tab gracefully.
- **Typst in WebAssembly** is possible but large; the web build may render the
  binder lazily.
