# Architecture

Status: proposed, 2026-11-27; sync and encryption added 2026-11-28. Decisions referenced as D-nn are in
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
| `crates/plan` | Floor plan geometry (D-130): whole-millimetre points, areas, angles, snapping, polygon checks, rooms from walls, lengths typed and shown. Pure; no I/O. |
| `crates/core` | Domain model, schedule engine, prioritisation, storage, operation log. No platform code. |
| `crates/api` | One JSON command surface over the core, shared by the Tauri app, the WebAssembly build and Android's notification receivers (D-63). File bytes travel beside the JSON (`call_upload`, `call_download`). |
| `crates/media` | Pixel work shared by Android and the web: photo downscaling, QR codes, the sticker sheet PDF, on-device OCR (D-72, D-74, D-75). |
| `models/ocr` | The bundled OCR models (CC BY-SA 4.0, D-74). |
| `crates/crypto` | Device keys, the household key chain, HPKE key envelopes, encrypted signed batches and files, the roster, signed requests, pairing, the recovery key (D-15, D-89 to D-94). |
| `crates/sync` | The sync client: push, pull, files, pairing, removal, recovery (D-97). Hybrid logical clocks and merge stay in the core. |
| `crates/binder` | The house binder (D-40 to D-43, D-80 to D-88): gathers one property's data, redacts it for the main volume, builds the editions as a document model, typesets them with an embedded Typst (fonts compiled in, no file system or network), fingerprints sections for reprints. |
| `crates/server` | The sync relay: stores and relays ciphertext only (Phase 6); later share links and paid extras. |
| `crates/cli` | A command-line front end over the core, for testing and for power users. |
| `app/` | Svelte interface and Tauri 2 shell (Android, iOS, desktop); `app/src-tauri/plugins/reminders` holds Android reminders (D-62). |
| `web/` | The API compiled to WebAssembly (SQLite on OPFS in a worker, D-61); the interface's web mode loads it. |
| `web-binder/` | The binder's typesetter (Typst) as a second WebAssembly module, fetched only on the first print; it turns a bundle made by `web/` into a PDF (D-80). |
| `crates/bridge` | `housewright-bridge`: the Home Assistant bridge, a headless member device with a token-protected local JSON API (D-117, D-122, D-124). |
| `integrations/home-assistant` | The Home Assistant custom integration (Python glue only, D-117), its tests, blueprints and the add-on (D-26). |
| `templates/` | Bundled template library (D-21); moving later to the separate public appliance library `holdTheDoorHoid/housewright-library` (CC BY 4.0, D-127 to D-129). |
| `tools/climate-build` | Builds the bundled ZIP climate table from NOAA and Census data (D-64). |

## Data model (core entities)

- **Household**: the sharing and encryption boundary. Has members and roles (D-23).
- **Property**: a house. A household can hold several (landlords).
- **Floor → Room**: places within a property. A floor may carry a plan
  (Phase 8): walls, openings (doors, windows, gaps), pins and labels are
  their own records on the floor; a room may have an outline; a floor may
  have a background picture to trace over. All optional.
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
- **Binder print**: append-only record of a binder PDF made: edition,
  volume, printed-on date and a fingerprint per section (D-82).
- **Attachment**: a file (manual, receipt, warranty, photo) referenced by
  content hash; the bytes live in a blob store beside the database (D-71),
  encrypted on the sync server from Phase 6 (plain on the device, D-95).
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

## The house binder (Phase 5)

```
Store ─► binder::gather ─► HouseData (every sensitivity flag kept; private
                              │        rating and member contacts dropped)
                              ├─► redact::Public::of ─► build::main_volume ─► Document (main)
                              └──────────────────────► build::sealed_volume ─► Document (sealed)
Document ─► check_main (no sensitive value in the text) ─► typeset::source
         ─► render (Typst, closed world: main.typ + img/<sha>.jpg + Lato) ─► PDF
Document.sections ─► fingerprint ─► track::status vs binder_print records
                                    ("3 sections changed since Nov 2")
```

- A *document* is a list of loose-leaf *sections* of *blocks* (headings,
  label/value pairs, tables, steps, photos, the emergency layout). It knows
  nothing about Typst, so tests read its strings directly.
- `typeset` writes a fixed prelude (page setup, footer "Printed <date> ·
  Water 3", one function per block) and then one call per block, with every
  piece of household text as an escaped Typst string literal.
- The API's `binder_pdf` download builds, checks, shrinks photos for print
  (`media::photo`), typesets, and records the print. On the web the same
  steps are split: `binder_bundle` (in `web/`) makes a ZIP of the Typst
  source, photos and a receipt; `web-binder/` typesets it; the worker then
  calls `record_binder_print` with the receipt. On Android the PDF goes
  to the phone's viewer through the existing plugin (D-79); on the web it
  opens in a new tab (or downloads).

## Sync and encryption (Phase 6)

```
local change ─► op log (core) ─► unsynced_ops ─► seal batch (crypto: XChaCha20-Poly1305,
                                                  signed Ed25519) ─► saved ─► POST /ops
server: verify signature, clock guard, store with seq + received time (ciphertext only)
GET /ops?after=cursor ─► per batch: roster check ─► verify ─► decrypt ─► author + clock
                         checks ─► Store::apply_synced_ops (per-field LWW, D-13)
attachments ─► blob name = HMAC(sha256) ─► encrypted blob PUT/GET (thumbnails eagerly)
roster: signed hash chain of devices (who may get the key); key chain: one key per
epoch, wrapped to each device with HPKE; removal = new roster version + new epoch
pairing: one-time code (secret never on the server) + committed safety number
recovery key: derives a recovery identity in the roster + an anchor key
```

- `crates/crypto` builds every byte the server stores; `crates/server` is a
  relay that checks signatures and the roster and understands nothing else;
  `crates/sync` is the client, behind a blocking `Transport` (ureq natively,
  synchronous XHR in the web worker), with no threads or timers of its own.
- The API exposes `sync_*` commands; the interface schedules syncs (D-97).
- Merging is unchanged: deterministic per field, so the user is never asked
  to resolve a conflict. Every receiver check uses only data every device
  sees the same way, so refusals cannot make devices diverge.
- Full design and threat model: [SECURITY.md](SECURITY.md).

## Home Assistant (Phase 7)

```
Home Assistant ── custom_components/housewright (Python glue: config flow,
   │               coordinator polling /api/v1/snapshot, to-do, calendar,
   │               sensors, buttons, services, sensor links, device import)
   │  HTTP + bearer token, 127.0.0.1:8788
   ▼
housewright-bridge (crates/bridge, Rust) ── Api (crates/api) ── Store, op log
   │  a member device with its own bridge.db and keys        (crates/core)
   ▼  sync (crates/sync), exactly like a phone
housewright-server ◄──► the household's other devices
```

- The bridge is an `Api` with a native transport plus a small axum router.
  Reads are projections built field by field (`crates/bridge/src/dto.rs`),
  never the core's own structures, so nothing sensitive can leak by a new
  field. Writes go through the core (`complete_task_for`, `snooze`,
  `record_reading`, `add_from_template`), so they are ordinary signed
  operations that sync and show in History "via Home Assistant".
- New in the core for it: the `triggered` schedule kind (events are
  readings of a per-item meter named after the event, D-118),
  `MemberKind::HomeAssistant` and `Completion.done_by` (D-122). The engine
  stays pure.
- The sync loop runs every 60 seconds and two seconds after a change;
  requests to the server time out after 15 seconds so Home Assistant is
  not kept waiting. One mutex guards the bridge, so an API call waits while
  a sync runs.
- The integration polls (`local_polling`, 60 seconds). Its tests run
  against the real bridge (D-126).
- The add-on runs the server and the bridge in one container
  (`integrations/home-assistant/addon/`).

## Floor plans (Phase 8)

```
housewright-plan (pure geometry, mm integers) ◄── core::plan (FloorPlan projection,
   area, snap, faces, units        │               delete_wall/floor/pin, place_pin)
                                   ▼                       │ op log: wall, opening,
app/src/lib/plan/geometry.ts ── same functions,            │ plan_pin, plan_label,
   (live readouts while drawing)   shared test vectors     │ room.outline, floor.background
                                                           ▼
api::plan (floors, floor_plan, save_*, plan_restore) ── Plans.svelte (SVG editor)
binder::plans ── one section per drawn floor (Typst draws from numbers only)
```

- The interface snaps and measures in TypeScript for speed, then sends
  whole-millimetre points; the core validates (outline simple, wall ends
  apart, door fits its wall and does not overlap another) and stores.
- Undo and redo (D-131) are the core's `plan_restore`: it re-applies only
  what one step changed, so other people's edits are kept.
- A thing's pin puts it in the room it is dropped in (D-132). The thing's
  page draws a small copy of its floor with the pin.
- Merge granularity and what a merge can leave behind: D-130 and
  [FILE-FORMAT.md](FILE-FORMAT.md#floor-plans-version-6).

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
- **Typst in WebAssembly** is large (see D-80 for the measured cost); it is
  part of the web build's one WebAssembly file.
