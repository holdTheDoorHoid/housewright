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
| `crates/api` | One JSON command surface over the core, shared by the Tauri app, the WebAssembly build and Android's notification receivers (D-63). File bytes travel beside the JSON (`call_upload`, `call_download`). |
| `crates/media` | Pixel work shared by Android and the web: photo downscaling, QR codes, the sticker sheet PDF, on-device OCR (D-72, D-74, D-75). |
| `models/ocr` | The bundled OCR models (CC BY-SA 4.0, D-74). |
| `crates/crypto` | Household keys, key wrapping, operation and file encryption (D-15). |
| `crates/sync` | Operation exchange with a server; hybrid logical clocks; merge (D-13). |
| `crates/binder` | House binder and reports through Typst (D-43). |
| `crates/server` | Sync relay, share links, paid extras. |
| `crates/cli` | A command-line front end over the core, for testing and for power users. |
| `app/` | Svelte interface and Tauri 2 shell (Android, iOS, desktop); `app/src-tauri/plugins/reminders` holds Android reminders (D-62). |
| `web/` | The API compiled to WebAssembly (SQLite on OPFS in a worker, D-61); the interface's web mode loads it. |
| `integrations/home-assistant` | Custom integration and add-on (D-26). |
| `templates/` | Community template library (D-21; may split into its own repo, Q-05). |
| `tools/climate-build` | Builds the bundled ZIP climate table from NOAA and Census data (D-64). |

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
- **Attachment**: a file (manual, receipt, warranty, photo) referenced by
  content hash; the bytes live in a blob store beside the database (D-71),
  encrypted from Phase 6.
- **Completion detail**: append-only costs and completion field values added
  after Done (D-76).
- **Template fields**: the household's own fields on a template (D-77).
- **Fact**: home facts sheet entries (D-27, D-78), in fixed groups, each
  marked normal or sensitive for the binder (D-41).
- **Profile**: the answers to the home questions, one per property (its id
  is the property's id). Every answer is optional ("Not sure"). Only counts
  are kept about the people at home (D-65).
- **Suggestion answer**: append-only record of Add / Not for us / Later on a
  weekly suggestion, so "Not for us" is permanent and merges cleanly (D-67).
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
  Climate comes from the ZIP code (D-64) or is entered directly.
- **Usage**: due when a meter passes a threshold since the last completion,
  optionally with a calendar cap ("every 300 hours or 6 months, whichever first").
- **One-off**: a single due date.

Due dates are *windows* (earliest, due, overdue-after), not instants, because
"sometime in October" is how home maintenance really works.

## Onboarding and growth (Phase 3)

```
answers ─► Profile (op log) ─► profile::plan_home ─► items + tasks (all off)
   │                                                     │
   └─ ZIP ─► climate::lookup_us_zip ─► Property.climate  ▼
                                      priority::starting_plan ─► 4-5 tasks on
weekly: Profile + items + switched-off tasks + answers + climate + today
        ─► suggest::propose ─► at most one card on Today
```

- `crates/core/src/profile.rs`: the `Profile` entity and `plan_home`, the
  pure mapping from answers to things (one per template) and obvious rooms.
- `crates/core/src/priority.rs`: `starting_plan`, the pure chooser of the
  first tasks (D-66).
- `crates/core/src/suggest.rs`: the pure just-in-time proposer (D-67); it uses
  the schedule engine's seasonal windows to decide what is in season.
- `crates/core/src/climate.rs`: the bundled ZIP climate table
  (`crates/core/data/us-zip-climate.bin`, D-64), built by `tools/climate-build`
  from public domain data ([DATA-SOURCES.md](DATA-SOURCES.md)).
- `crates/core/src/onboard.rs`: the store-level glue: save the profile, add
  missing things (never change existing ones), switch on the plan, answer a
  suggestion.

## Records (Phase 4)

```
photo/PDF ─► media::photo (shrink, thumbnail) ─► BlobStore.put ─► sha256
                                                     │
                Attachment {sha256, thumb, about} ◄──┘  (op log record)
delete ─► tombstone ─► blobs::collect_garbage (drops unused blobs)
full backup = JSON export + blobs/<sha256> in one ZIP (core::backup)

label photo ─► media::ocr (ocrs + rten, bundled models) ─► lines
            ─► core::labels::read_plate ─► brand/model/serial candidates
            ─► the person checks and saves (never automatic)
sticker: housewright://item/<id> + HW-XXXXX code ◄─ core::labels
         QR drawn by media::qr, sheet by media::sheet (PDF);
         scanned by media::qr (rqrr) or the phone's camera (Android link)
```

- `BlobStore` implementations: `DirBlobs` (a folder; Android, desktop),
  `SqliteBlobs` (a second SQLite file; the web on OPFS), `MemoryBlobs`
  (tests, the web's in-memory fallback). Phase 6 wraps the trait with
  encryption.
- Sensitive values (facts, fields, serials) are not sent to the interface
  until it asks `reveal_*`, which only the owner may do. They never appear
  in History or reminders.
- On Android, files the WebView cannot handle (PDFs, backups) are written to
  the app cache and opened or shared through the plugin (D-79).

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
