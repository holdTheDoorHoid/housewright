# Roadmap

Phases are ordered so each one gives something usable and tests the riskiest
assumption early. Status as of 2026-11-28.

| # | Phase | Delivers | Status |
|---|---|---|---|
| 0 | Foundations | Interview, decision log, research, architecture | done |
| 1 | Core | Rust domain model, schedule engine (all kinds), SQLite storage, operation log, CLI, tests | done (merged 2026-11-27, PR #1); see below |
| 2 | App shell | Tauri 2 + Svelte on Android and web; items, tasks, one-tap completion; **prove local notifications with actions on Android** | done on branch `phase-2-app-shell` (PR #2): web verified in a browser; Android reminder proof passed on the emulator 2026-11-27 (see below) |
| 3 | Onboarding + templates | 10-question questionnaire, generic template set, prioritisation (life safety first), start-small activation | built on branch `phase-3-onboarding` (2026-11-27): Rust, web (Playwright) and Android emulator walk-through pass; see below |
| 4 | Records | Photos and documents, custom fields, home facts sheet, QR labels, costs | built on branch `phase-4-records` (2026-11-27): Rust, web (Playwright) and Android emulator checks; see below |
| 5 | House binder v1 | Typst binder: emergency page, main volume, sealed volume, change tracking for reprints | built on branch `phase-5-binder` (2026-11-28): all four editions (D-42 brought forward from Phase 9), Rust and web (Playwright) checks; Android emulator checks pass (one bug found and fixed); see below |
| 6 | Sync + encryption | Self-hosted server, end-to-end encryption, multi-device, members and roles, recovery key | built on branch `phase-6-sync` (2026-11-28): crypto, server, client, interface; Rust, web (Playwright, two browser profiles) and Android emulator checks; awaits an independent security review; see below |
| 7 | Home Assistant | Tasks as HA to-do/calendar/sensors, HA meters and events trigger tasks, device import, server as add-on | built on branch `phase-7-home-assistant` (2026-11-28): Rust bridge, Python integration (D-117), add-on files; Rust, Python (against the real bridge, HA 2026.10.4) and browser (Playwright) checks; the add-on image is not built or run yet; see below |
| 8 | Floor plans | Stage A: rooms on a grid, floors, item pins. Stage B: measured walls, angles, windows, area, PDF | built on branch `phase-8-floor-plans` (2026-11-28): Stage A and Stage B, binder pages, Rust, web (Playwright) and Android emulator touch checks; see below |
| 9 | Contractors and alerts | Contractor records and job history, warranty expiry, CPSC recall checks, insurance export, binder editions (handover, succession) | |
| 10 | iOS | Apple account, Mac/cloud builds, App Store | |
| 11 | Hosted + extras | Paid hosting, opt-in disclosure extras (email/SMS reminders, recalls), AI label reading, share links for contractors and tenants | |
| 12 | Landlord depth | Multi-property dashboards, tenant problem reports, per-property costs | |

Label scanning on the device (D-29) landed with Phase 4 on Android and the web.
Opt-in research data (D-32) was planned with Phase 6; it was not built (Q-11).

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

**Android reminder proof: passed** on the Android 15 emulator, 2026-11-27,
via the emulator-only `~/Android/emu-adb` helper. All four checks pass:
(a) a reminder fires with the app in the background, (b) it fires after the
process is killed (Android restarts the app for it), (c) Done in the
notification records the completion with the app closed and it shows in
History, (d) the plan is rescheduled after a reboot. Evidence and a summary:
[phase-2-evidence/README.md](phase-2-evidence/README.md). The proof found one
bug, now fixed: Done on a *test* reminder logged a real completion. Still
untested: a real phone with aggressive battery saving (Samsung, Xiaomi), and
force-stop, which by Android design cancels alarms until the app is opened.

Not built in Phase 2: photos ("Done + photo" waits for attachments in
Phase 4), the weekly digest (P5), adding a calendar entry when rescheduling
(P6), reminders on the web when the page is closed (Phase 11), iOS
(Phase 10), and the full questionnaire (Phase 3).


## Phase 3 notes

Built (decisions D-64 to D-69):

- **Home questions**: name and home name, then ten questions, one per
  screen, each skippable ("Not sure"), with progress and Back. Answers are a
  `Profile` entity per property, through the operation log. "Redo the home
  questions" in Settings only adds.
- **Starting plan**: everything the answers imply is recorded with its tasks
  switched off; four tasks are switched on (five with children or older
  adults), alarms first, one per thing, routine before long-cycle, one place
  for the top water or fire task. The screen shows each task's why and
  source and "Your home, so far: N things recorded"; the full list is one tap
  away but never the landing page.
- **One suggestion a week** on Today (Add / Not for us / Later), seasonal via
  the existing windows; "Not for us" is permanent.
- **Climate from ZIP, offline**: a 199 KB table of NOAA 1991-2020 frost dates
  and hard-freeze flags for every US ZCTA, built reproducibly by
  `tools/climate-build` from public domain data ([DATA-SOURCES.md](DATA-SOURCES.md)).
- **Templates**: 45 generic templates in eight fixed categories, with a
  quality test.
- Storage schema 2 and export version 2; Phase 2 exports still import
  (tested with a fixture written by the Phase 2 code). `Store::batch` writes
  onboarding in one transaction (D-70).

Verified: Rust format, clippy (native and wasm), tests; svelte-check;
vitest; Playwright on the production web build (first run, all ten
questions, starting plan, Today with only the switched-on tasks and one
suggestion, the full suggested list, Done, add a thing, redo the questions,
second tab, reload); and a walk-through on the Android 15 emulator with
screenshots in [phase-3-evidence/README.md](phase-3-evidence/README.md).

Not built in Phase 3: room-by-room walks with label photos (D-24's second
half; Phase 4 with attachments), editing a thing's template fields after
onboarding (for example a boiler whose fuel was not known), a "weekly in
season" schedule kind, frost dates for places outside the US, and correcting
the looked-up frost dates by hand in the interface.


## Phase 4 notes

Built (decisions D-71 to D-79):

- **Photos and documents**: stored once by content hash beside the
  database (a folder on Android, a second SQLite file in OPFS on the web),
  behind a `BlobStore` trait ready for Phase 6 encryption. Photos are shrunk
  on the device to 1600 px JPEG with a thumbnail (camera metadata, including
  location, dropped); PDFs kept as-is up to 25 MB. On a thing ("Add a photo
  or PDF", with a kind), on a completion (after Done), on a fact. Deleting
  is a tombstone, then unused blobs are removed. PDFs open in a new tab on
  the web and in the phone's viewer on Android.
- **Full backup**: one ZIP of the JSON export plus every file; restore from
  the first-run screen accepts the ZIP or a JSON export. Export version 3;
  Phase 2 and Phase 3 exports still import.
- **Custom fields**: an edit form for a thing's details and template fields,
  fields of your own on one thing or every thing of that kind, all six
  types, sensitivity from the name by default. Changing a field offers
  conditional tasks (never switches them on) and lists ones that no longer
  apply.
- **Done, then details**: the Done toast offers "Add details": a note,
  photos, costs (parts, labour, contractor, other) and the task's own
  readings. Nothing is asked before Done.
- **Home facts sheet** (Home tab): six groups, one suggested question at a
  time from the home profile, sensitive values hidden until revealed (owner
  only), never in History or reminders.
- **Costs**: totals by year, thing and home in the household's currency.
- **QR stickers**: a code per thing (`HW-` and five characters), a PDF sheet
  with a configurable grid, the phone's camera opens `housewright://item/…`
  links on Android, "Scan a sticker" reads a photo on any platform, and the
  code can be typed.
- **Label reading** on the device with ocrs (models bundled, CC BY-SA 4.0):
  brand, model and serial candidates, a matching template, always checked
  by the person before saving.

Sizes (measured 2026-11-27):

| | Before (Phase 3, 5dd1ff2) | After | Change |
|---|---|---|---|
| Web: WebAssembly core | 4.30 MB (1.62 MB gzip) | 8.56 MB (3.22 MB gzip) | +4.26 MB (+1.60 MB gzip), mostly the OCR runtime |
| Web: OCR models, fetched on first label read only | none | 12.2 MB (11.4 MB gzip) | +12.2 MB, only when used |
| Android native library (x86_64 debug, stripped) | not rebuilt | 48.1 MB | about +18 MB: 12.2 MB models + about 5.8 MB of library code (estimate; see `phase-4-evidence/README.md`) |

Timings: reading the synthetic rating plate took about 3 s natively (test
build, optimised dependencies), about 16 s in Chrome the first time
(including fetching and loading the 12 MB of models), and about 24 s on the
Android emulator in a debug build (first use, including loading the
models).

Verified: Rust format, clippy (native and wasm), tests (blob dedupe and
garbage collection, photo downscaling, ZIP round trip, QR encode/decode,
rating-plate heuristics on six synthetic plates, OCR on a synthetic plate
photo, sensitive values absent from History and reminders, older exports);
svelte-check; vitest; Playwright on the production web build; and the
Android emulator checks in [phase-4-evidence/README.md](phase-4-evidence/README.md).

Not built in Phase 4: "Done + photo" as a notification button (photos are
added in the app after Done), attachments on contractors in the interface
(the model and API accept them), the room-by-room walks of D-24, offline
label reading on the web before the first use, item fields that look like
sizes copied onto the facts sheet, and syncing blobs between devices
(Phase 6).


## Phase 5 notes

Built (decisions D-80 to D-88):

- **`housewright-binder`**: gathers one property's data, takes everything
  sensitive out for the main volume (the main builders can only see a
  redacted copy), builds the editions as a document model, and typesets
  them with Typst embedded (two Lato fonts compiled in; no files, no
  network; every piece of household text written as an escaped string
  literal). Each section starts on a new page with its own numbers and the
  footer "Printed Nov 28, 2026 · Water 3".
- **Editions**: the emergency page (one page, always, Letter or A4: shutoffs
  with their photos, who to call, gas smell, carbon monoxide alarm, water
  leak, no power), the household binder and its sealed volume ("SEALED —
  keep in a safe place", with a placeholder page for the Phase 6 recovery
  key), the handover edition (no names, costs optional, transferable
  warranties, no sealed volume) and the succession edition (accounts and
  utilities to transfer, as tick lists, numbers in the sealed volume). The
  handover and succession editions were planned for Phase 9; they are done.
- **Stays current**: every print records a fingerprint per section; the
  screen says how many sections changed since the last print and prints
  just those. After the first print a yearly "Reprint the house binder"
  task exists, switched off, offered once.
- **Interface**: Home, then House binder: pick an edition, see the
  sections with "changed" markers, print the main volume and the sealed
  volume as separate PDFs, say where the sealed volume is kept, keep a
  short "Who to call" list (emergency contacts go on page 1) and the
  house's quirks. A thing's page has "Quirks only you know".
- **Web**: Typst is a separate WebAssembly module fetched only on the
  first print (D-80); Android and the desktop typeset in-process.
- Storage schema 4 and export version 4 (`binder_print`, contractor
  `emergency`); Phase 3 exports still import and print.

Sizes (measured 2026-11-28):

| | Before (Phase 4) | After | Change |
|---|---|---|---|
| Web: main WebAssembly module | 8.56 MB (3.23 MB gzip) | 9.41 MB (3.46 MB gzip) | +0.85 MB (+0.24 MB gzip): the binder's own code, without Typst |
| Web: binder module (Typst), fetched on the first print only | none | 30.9 MB (11.6 MB gzip) | only when someone prints |
| Web, for comparison: Typst inside the main module | | 38.3 MB (14.2 MB gzip) | +29.8 MB (+11.0 MB gzip); why it was split (D-80) |
| Android native library (x86_64 debug, stripped) | 48.1 MB | 114.6 MB | about +66 MB in a debug build (Typst's symbols alone are about 29 MB). A release build (`opt-level = "s"`, LTO) was not measured; the web module above suggests Typst costs roughly 25 to 30 MB there too. |

Making the household binder for the emulator's test household (about
twenty things, few photos) took about 2 s on the emulator in a debug build, from
the tap to the PDF viewer. The web's first print also fetches the 11.6 MB
module; it was not timed separately.

Verified: Rust format, clippy (native and wasm), tests (no sensitive value
in any main volume: document model, laid-out pages and `pdftotext`
output; the sealed volume holds every one; private rating never printed;
handover anonymised; emergency page one page when crowded, on Letter and
A4; hostile strings print literally and change nothing; a section's
fingerprint changes only with its content; reprinting only changed
sections; only the owner prints the sealed volume; the web's
bundle-render-record path); svelte-check; vitest; Playwright on the
production web build (every edition and the sealed volume, the change
marker and a reprint). PDFs of a demo household were looked at page by
page; see [phase-5-evidence/README.md](phase-5-evidence/README.md).

Not built in Phase 5: editing a contractor's licence, insurance or private
rating in the interface (Phase 9), a screen per room for room quirks (the
data model and the binder support them), choosing which property to
print when there are several (the API takes a property id; the screen
prints the first), printing manuals themselves into the binder, and
non-Latin scripts in the binder (Lato covers Latin, Greek and Cyrillic;
other text would print as empty boxes).


## Phase 6 notes

Built (decisions D-89 to D-98; design and threat model in
[SECURITY.md](SECURITY.md); running a server in
[SELF-HOSTING.md](SELF-HOSTING.md)):

- **`housewright-crypto`**: device keys (Ed25519 + X25519), the household
  key chain with epochs, HPKE key envelopes, encrypted and signed operation
  batches (XChaCha20-Poly1305), encrypted files with keyed-hash names, the
  signed roster hash chain, signed requests, pairing codes with a committed
  six-digit safety number, the printable recovery key and its anchor.
- **`housewright-server`**: one binary, axum + SQLite, stores and relays
  only ciphertext; device-signed requests with a replay window; the roster
  checked as clients check it; rate limits on the open endpoints; body
  limits; TLS (rustls) or behind a proxy; a systemd unit and a Dockerfile.
- **`housewright-sync`**: resumable push (sealed and saved before sending),
  paged pull with every receiver check, files up and down (big ones when
  opened), pairing, removal with key rotation, recovery-key restore and
  replacement. **The Phase 1 clock gap is closed** (D-93): operations more
  than five minutes ahead of the server's receive time are refused, the
  same way on every device, and the person is told the clock looks wrong.
- **Interface**: Settings, Sync and devices (off by default; your own
  server; hosted "coming later"; the recovery key shown once, with a
  confirmation and "print the sealed volume with this key"; devices with
  last seen; add a device or person; remove; sync now; a new recovery key;
  stop syncing here). First run: join a household, or restore with a
  recovery key. The sealed volume's recovery page now prints the server
  and, on the print made at setup, the key.
- Storage schema 5 (`sync_state`, `sync_remote`); the export format is
  unchanged (version 4): keys are never exported (FILE-FORMAT.md).

Sizes (measured 2026-11-28): the web's main WebAssembly module is
10.16 MB (3.77 MB gzip), up from 9.41 MB (3.46 MB gzip), about +0.75 MB
for the cryptography and the sync client. The Android library gains rustls,
ring and ureq (see `phase-6-evidence/README.md`).

Verified: Rust format, clippy (native and wasm), tests including the
security tests listed in SECURITY.md (three-device convergence property
test with offline periods, dropped connections and server restarts; no
plaintext in the server's files; tampering, moving, replay, forged
authorship, removed devices, a middleman with the pairing code, the clock
guard, recovery restore); svelte-check; vitest; Playwright on the
production web build with a real server and two browser profiles; and the
Android emulator paired with a browser profile
([phase-6-evidence/README.md](phase-6-evidence/README.md)).

Not built in Phase 6: background sync on Android (sync runs while the app
is open), scanning the pairing QR with the new device's camera (the code is
typed, or the QR text pasted), Android Keystore / WebCrypto wrapping of the
device keys (D-95), cryptographic limits for limited members (D-98),
detecting a server that forks or withholds roster versions, reporting gaps
in a device's batch numbers, re-stamping changes made with a clock far
ahead, the hosted service and opt-in research data (Phase 11, Q-11).

Independent security review (2026-11-28,
[security-reviews/2026-11-28-phase6-review.md](security-reviews/2026-11-28-phase6-review.md)):
verdict "ship after fixes". Every finding was fixed or mitigated the same
day (D-99 to D-105, regression tests in `crates/sync/tests/review.rs`); the
remaining limits are listed in SECURITY.md. The reviewer's verification
pass found new problems in the fixes (N1 to N6, L6); those were fixed too
(D-106 to D-111, `crates/sync/tests/verify.rs`). The Android emulator pairing
was not re-run after the fixes (not enough disk for an Android build); the
Rust and browser tests cover the protocol changes.

## Phase 7 notes

Built (decisions D-117 to D-126; owner questions Q-12 to Q-14; how it
fits in [ARCHITECTURE.md](ARCHITECTURE.md#home-assistant-phase-7); what
Home Assistant can see in [SECURITY.md](SECURITY.md); setup in
[integrations/home-assistant/README.md](../integrations/home-assistant/README.md)):

- **`housewright-bridge`** (`crates/bridge`): a headless household member
  device. Pairs with the app's code and safety number (from Home
  Assistant's setup, or `housewright-bridge join`), refuses an owner's
  device code, keeps its own SQLite and keys, syncs every minute and soon
  after changes. Local JSON API on 127.0.0.1:8788 with a bearer token
  (constant-time compare): status, snapshot, tasks, items, members,
  complete (for a named person), snooze, meter readings (throttled,
  delta, counter restarts), events, device match and import, pairing,
  sync. `housewright-bridge demo` makes a sample household for trying it.
- **Core**: the `triggered` schedule kind (D-118) with leak and
  low-battery tasks in five templates; `MemberKind::HomeAssistant` and
  `Completion.done_by` (D-122); History says "via Home Assistant"; usage
  baselines at the reference instant (D-121). Export version 5.
- **Integration** (`integrations/home-assistant/custom_components/housewright`):
  config flow with pairing and reauth, a Maintenance to-do list and
  calendar, a state sensor per task, a "Tasks ready to do" summary
  sensor, "Mark done" buttons, four services, options for the poll
  interval and default person, device import, sensor links to meters and
  events, diagnostics with redaction; two blueprints; `hacs.json`.
- **Add-on** (`integrations/home-assistant/addon/`): server and bridge in
  one container, data in `/data`, registration token on.
- **App**: "Home Assistant" when adding a device or person; the bridge
  marked "Home Assistant bridge" in the device list; a "Connect Home
  Assistant" page (from Settings and from Sync and devices).

Verified: Rust format, clippy (native and wasm), cargo deny, tests
(bridge: token on every route, nothing sensitive in any answer, every
path, pairing and convergence with an app device through the real
server); Python: ruff, mypy, 22 tests with Home Assistant's own test
harness against the real bridge; a smoke start of a real Home Assistant
2026.10.4 core loading the integration (23 entities); svelte-check,
vitest; Playwright (`app/tests/ha.spec.ts`): a household made in the
browser pairs the bridge, the bridge completes a task for Ana and sends
meter readings, and the browser's History shows "via Home Assistant" and
the usage task comes due.

Not built or not checked: the add-on image was not built (no Docker here)
nor run on Home Assistant OS; HACS and add-on store publishing (Q-12);
push updates (the integration polls); custom Assist intents (D-125; the
to-do list's built-in sentences were not tried with a voice pipeline);
NFC tags; Android (nothing changed there beyond the shared interface).

## Phase 8 notes

Built (decisions D-130 to D-135; owner questions Q-15 to Q-17; how it fits
in [ARCHITECTURE.md](ARCHITECTURE.md#floor-plans-phase-8); the stored
shapes in [FILE-FORMAT.md](FILE-FORMAT.md#floor-plans-version-6)). The
owner's appliance-library decisions (D-127 to D-129) were recorded in the
same branch; the library is not wired into the app.

- **`housewright-plan`** (`crates/plan`): pure geometry in whole
  millimetres: shoelace areas, perimeters, wall lengths and angles,
  polygon checks, point-in-room, label points, snapping (corners, 90° and
  45°, a fractional grid), rooms found from walls (ends and T joints),
  rectangles and L shapes, and lengths typed and shown in feet and inches
  or metres. Unit tests, property tests (area and perimeter invariant
  under moving, turning, restarting and reversing an outline; snapping
  idempotent and close; lengths surviving format and parse), and shared
  vectors also run by the interface's tests.
- **Core**: new kinds `wall`, `opening`, `plan_pin`, `plan_label` (one
  record each, so concurrent edits to different ones merge), `room.outline`
  (one field), `floor.background`, `fact.pin`, `household.units`; schema
  6, export version 6 (older exports and the Phase 2/3 fixtures import
  unchanged). `core::plan`: the floor projection (forgiving about what a
  merge leaves behind), pins that put things in rooms, deleting a wall,
  pin or floor cleanly. Convergence property test for concurrent plan
  edits, and a sync test of two people editing one plan offline through
  the real server.
- **Commands**: floors, floor_plan, save_room_shape, remove_room_shape,
  save_walls, delete_wall, save_opening, delete_opening, save_pin,
  delete_pin, save_label, delete_label, set_background, set_units,
  plan_restore (undo/redo); a thing's detail carries its pin; facts carry
  a pin link; files can be attached to a floor.
- **Editor** (`app/src/screens/Plans.svelte`, SVG): floor switcher; Room
  (rectangle or L by dragging, with size and area as you drag; name it or
  pick an existing room); Walls (corner by corner, live length, direction
  and corner angle, typed lengths, close the shape, closed spaces offered
  as "Tap to name"); Door and Window on a wall with typed width and
  position and a flippable swing; Pin (a thing or a named spot, private
  option, link to a shutoff fact); Label; Move (pan, pinch, scroll zoom,
  drag corners, room corners and pins); trace over a photo (dimmed,
  movable, real-life width typed); undo/redo; keyboard shortcuts; every
  item also in lists under the drawing; units setting; "Print this floor".
  A thing's page shows a small plan with its pin, or offers to place it
  once a floor is drawn.
- **Binder**: one page per drawn floor (rooms with areas, walls, doors
  with swings, windows, numbered pins, labels, scale bar), a room-area
  table with the total, a pin legend; shutoff pins dark and named on the
  emergency page ("Basement plan, pin 2"); private pins, pins of private
  things and labels marked private stay out of the main volume.

Verified: Rust format, clippy (native and wasm), tests (plan, core,
convergence, sync, api including binder PDFs read back with `pdftotext`
and looked at with `pdftoppm`); svelte-check, vitest (geometry and
readouts against the shared vectors, the plan screen); Playwright
(`app/tests/plans.spec.ts`): two rooms dragged and named, a pin that
moves its thing into the kitchen, a second floor with walls tapped and
one typed (12' 6" exactly), the closed space named, a door added, undo
and redo, a traced picture, reload, the thing's plan thumbnail, and the
floor printed as a PDF in the browser; a phone-sized pass. Android
emulator (debug x86_64 APK, `input swipe`/`tap`): a room dragged out by
touch and named, a thing pinned (it moved into the kitchen), one-finger
pan in Move; screenshots in `docs/phase-8-evidence/android/`. Screenshots
and the PDF are in `docs/phase-8-evidence/`.

Not built or not checked: LiDAR scanning (D-25, later); tracing over a
PDF (photos and screenshots only, Q-17); a drawn plan snippet on the
emergency page (Q-16); walls crossing in an X are not split into rooms;
pinch zoom was not exercised by any test (adb input cannot do two
fingers; mouse, wheel, buttons and one-finger touch were).

