# Housewright export format (version 7)

Housewright can write everything about a household to one JSON file and read it
back without losing anything. The format is documented here so that your data is
never locked in: another program can read it, and a script can produce it.

`hw export FILE` writes it; `hw import FILE` reads it into a new, empty
database. The code is `crates/core/src/export.rs`.

## Top level

```json
{
  "format": "housewright-export",
  "version": 6,
  "exported_at": "2026-11-27T17:00:00Z",
  "household_id": "01a21f65-3c12-72cb-92ba-a9ba73a49591",
  "entities": { "household": [ ... ], "member": [ ... ], "task": [ ... ] },
  "operations": [ ... ]
}
```

| Key | Meaning |
|---|---|
| `format` | Always `"housewright-export"`. |
| `version` | `6` (since Phase 8; `5` in Phase 7, `4` in Phase 5, `3` in Phase 4, `2` in Phase 3, `1` before). A reader must refuse versions newer than it knows. |
| `exported_at` | When the file was written (RFC 3339, UTC). |
| `household_id` | The household this file holds. |
| `entities` | The current state, grouped by kind. |
| `operations` | The full change history. Optional (see "Importing"). |

## Entities

`entities` maps a kind name to a list of rows, sorted by `id`. Kinds with no
rows are left out. Each row is:

```json
{ "id": "01a21f65-...", "deleted": true, "data": { ... } }
```

`deleted` appears only when true: deleted things are kept as tombstones so that
the history stays consistent. `data` is the entity's fields. A field that is not
set is absent (never `null`).

Identifiers are UUIDv7 strings, except templates, whose id is their slug
(`"water-heater"`). Dates are `YYYY-MM-DD`. Instants are RFC 3339 in UTC.
Money is `{ "minor": 12999, "currency": "USD" }` (minor units, so $129.99).

### Sensitivity

Anything that could be a code, an account detail or the location of valuables
carries `"sensitivity": "normal"` or `"sensitive"` (D-41). It appears either on
the entity itself (`fact`, `item`, `attachment`) or on a wrapped value:

```json
"serial": { "value": "SN1234", "sensitivity": "normal" }
```

The house binder uses this flag to decide which volume a value is printed in.
A program reading this file should treat `sensitive` values with care.

### Kinds and their fields

Required fields are in **bold**.

| Kind | Fields |
|---|---|
| `household` | **id**, **name**, **time_zone** (IANA), currency (ISO 4217; absent means USD), units (version 6: `imperial` or `metric`, how floor plans show lengths; absent means imperial) |
| `member` | **id**, **name**, **role** (`owner`, `member`, `limited`), kind (version 5: `home_assistant` for the Home Assistant bridge, D-122; absent for people), contact (wrapped), reminders `{quiet_hours: {start, end}, maintenance_window: {days, start, end}}` (times as `"HH:MM:SS"`, days as weekday names; D-59) |
| `property` | **id**, **name**, **time_zone**, **climate** `{hemisphere: north/south, first_frost_doy, last_frost_doy, hard_freeze}`, address (wrapped) |
| `floor` | **id**, **property**, **name**, level, background (version 6: `{attachment, at: {x, y}, width, opacity}`, a picture of an existing plan to trace over; `at` is its top-left corner and `width` its real-life width, in plan millimetres; `opacity` 0 to 100) |
| `room` | **id**, **property**, **name**, floor, outline (version 6: the room's shape on its floor's plan, a list of `{x, y}` corners in plan millimetres; a simple polygon) |
| `item` | **id**, **property**, **name**, room, parent (item id), category, make, model, serial (wrapped), purchased, installed, warranty `{expires, months, register_url, provider, transferable, reference (wrapped)}` (version 7: `months` is the warranty's length when given as one, counted from the earlier of `installed` and `purchased`, and `expires` is then worked out from it; `register_url` is an https link), location_note (wrapped), template, extra_fields, custom, sensitivity, price, value (version 7: Money, what it cost and the household's own estimate of what it is worth now) |
| `template` | the template, exactly as in `templates/README.md` |
| `task` | **id**, **property**, **title**, **owner** (member id), **priority**, **schedule**, **start** (date), item, room, window, steps, why `{text, source}`, minutes, completion_fields, template `{template, task}`, active, paused, not_applicable |
| `completion` | **id**, **task**, **by**, **at**, notes (wrapped), cost, values, contractor, attachments, voids, done_by (version 5: the member it was done for, when `by` is the Home Assistant bridge) |
| `snooze` | **id**, **task**, **by**, **at**, **until** (date), reason |
| `meter` | **id**, **key**, **name**, **unit**, item |
| `meter_reading` | **id**, **meter**, **at**, **value**, **by** |
| `contractor` | **id**, **name**, trades, phone, email, licence and insurance `{number (wrapped), expires}`, private_rating (1-5), notes (wrapped), emergency (version 4: `true` puts them on the binder's emergency page), company, website, service_area, would_hire_again (`true`/`false`), private_notes (version 7). `private_rating`, `would_hire_again` and `private_notes` are the household's private opinion (D-136): never printed, never sent to Home Assistant or a server extra. They are in a full export, which is the household's own data. |
| `attachment` | **id**, **sha256** (hex), **name**, **media_type**, **size**, **kind** (`manual`, `receipt`, `warranty`, `photo`, `other`), **about** `{kind, id}`, sensitivity, thumb_sha256 (hex, photos), width, height (pixels), by (member id), at (instant) |
| `note` | **id**, **about** `{kind, id}`, **by**, **at**, **text** (wrapped). A note about a `property`, `room` or `item` is a "quirk only you know", printed in the house binder (main volume, or the sealed volume when sensitive). |
| `fact` | **id**, **property**, **category** (`shutoffs`, `sizes`, `finishes`, `access`, `utilities`, `other`), **label**, **value**, **sensitivity**, key (the suggested question it answers, e.g. `water_main`; `sealed_volume_location` is where the binder's sealed volume is kept, always normal so the main binder can print it), room, pin (version 6: the `plan_pin` showing where it is) |
| `profile` | (version 2) the answers to the home questions, one per property: **id** (the property's id), home_type, built, zip (wrapped), heating `[...]`, cooling `[...]`, water_heater, water_supply, sewer, features `{basement: true, ...}`, smoke_alarms, smoke_power, co_alarms, **people** `{adults, children, older_adults, pets}`. A missing answer means "not sure". Values are the snake_case names in `crates/core/src/profile.rs`. |
| `suggestion_answer` | (version 2) **id**, **property**, **key** (`ask:<template id>` or `task:<task id>`), **answer** (`added`, `not_for_us`, `later`), **by**, **at** |
| `completion_detail` | (version 3) **id**, **completion**, **by**, **at**, costs `[{kind: parts/labour/contractor/other, amount: Money}]`, values (as completion `values`), contractor and contractor_cleared (version 7: who did the job; the latest detail that has either decides, `contractor_cleared: true` meaning "not a contractor"; otherwise the completion's own `contractor`). Added after Done. For one completion, the latest detail that has `costs` gives its costs (an empty list clears them); values are overlaid in order. |
| `binder_print` | (version 4) **id**, **property**, **edition** (`emergency`, `full`, `handover`, `succession`), **volume** (`main`, `sealed`), **by**, **at**, **printed_on** (date), **sections** `{section key: fingerprint}`. A record that a binder PDF was made, holding a hash of what each section printed, so the app can say which sections changed since. The latest record that includes a section counts for it. |
| `wall` | (version 6) **id**, **floor**, **a**, **b** (its two ends, `{x, y}` in plan millimetres), thickness (millimetres; absent means 115) |
| `opening` | (version 6) **id**, **floor**, **wall**, **kind** (`door`, `window`, `gap`), **offset** (millimetres from the wall's `a` end to its near edge), **width**, swing (doors: `{hinge: a/b, side: left/right}`, the side seen looking from `a` to `b`) |
| `plan_pin` | (version 6) **id**, **floor**, **at** (`{x, y}`), item (a thing), label, wall (when it sits on one), sensitivity. A pin holds a thing, a label, or both. |
| `plan_label` | (version 6) **id**, **floor**, **at**, **text**, sensitivity |
| `recall_dismissal` | (version 7) **id**, **item**, **recall** (the CPSC recall number), **by**, **at**. "Not mine": this recall does not apply to this thing. A record; deleting it (a tombstone) shows the match again. |
| `template_fields` | (version 3) **id** (a template id), **fields** (field definitions the household added to every thing made from that template) |

`schedule` uses the same shapes as templates, plus two that only a real house
can have: `{"kind": "one_off", "due": "2027-03-01"}` and an `anchor` date on
`fixed`. Version 5 adds `{"kind": "triggered", "event": "leak"}` (also in
templates, D-118): due when the event is reported on the task's item; each
event is a `meter_reading` of that item's meter whose `key` is the event. Field definitions (`extra_fields`, `completion_fields`) use the template
field shape. Custom values (`custom`, `values`) map a field key to:

```json
{ "value": { "type": "choice", "value": "gas" }, "sensitivity": "normal" }
{ "value": { "type": "number", "value": { "value": 50.0, "unit": "gal" } }, "sensitivity": "normal" }
```

where `type` is `text`, `number`, `date`, `yes_no`, `choice` or `photo` (an
attachment id).

Completions, snoozes, meter readings, attachments, notes, suggestion
answers, completion details, binder prints and recall dismissals are records: they are never edited. A completion logged by mistake is cancelled by a later
completion whose `voids` is the mistaken one's id.

Attachments hold only a content hash (and, for photos, the hash of a small
thumbnail). The JSON export does not contain file contents; the full backup
does (below).

### Floor plans (version 6)

Plan coordinates are whole millimetres: x points right and y points
**down** (as on a screen). Each wall, opening, pin and label is its own
entity, so concurrent edits to different ones merge cleanly; within one,
fields merge last-writer-wins like any other entity (a wall's `a` and `b`
are separate fields). A room's `outline` is one field: concurrent
reshapes of the same room resolve to the later outline, whole (D-130).
A reader drawing a plan should skip an opening whose wall is missing or
deleted, and clamp an opening onto a wall that has become too short for
it (offset and width within the wall's length).

## Operations

The history is the list of every change, in merge order:

```json
{
  "id": "01a21f65-3c19-72d0-9d64-dfb753d77f15",
  "hlc": "000001a21ce06e00.00000000",
  "author": "01a21f65-3c12-72cb-92ba-a9b958a722fc",
  "kind": "household",
  "entity": "01a21f65-3c12-72cb-92ba-a9ba73a49591",
  "field": "name",
  "value": "The Testers"
}
```

| Key | Meaning |
|---|---|
| `id` | Unique id of this change. |
| `hlc` | Hybrid logical clock: 16 hex digits of milliseconds since 1970, a dot, 8 hex digits of counter. Sorting the strings sorts the clock. |
| `author` | The member who made the change. |
| `kind`, `entity` | What was changed. |
| `field` | A field name, or `$record` (a whole record written once), or `$deleted` (a tombstone). |
| `value` | The new value as JSON. `null` clears the field. |

To rebuild state from operations (the merge rules, D-13):

1. Ignore an operation whose `id` you have already applied.
2. Order operations by `hlc`, then `author`, then `id` (all as strings).
3. For an ordinary field, the latest operation in that order wins.
4. For `$record` (completion, snooze, meter_reading, attachment, note,
   suggestion_answer, completion_detail, binder_print), the
   record is the whole `value`, written once: if a second, different record
   claims an id that already has one, it is ignored (D-104). A reader
   rebuilding state should keep the first record it applies; replaying in
   clock order is what the app's import does.
5. `$deleted` marks the entity deleted forever; later edits do not bring it
   back. Completions, snoozes, suggestion answers, completion details and
   binder prints cannot be deleted; such operations are ignored. Attachments can be.
6. Field operations on record kinds, and `$record` on other kinds, are ignored.

The result does not depend on the order in which the operations are applied.

## Importing

`hw import` needs an empty database. Then:

- **With `operations`:** the history is replayed exactly, and the resulting state
  must equal the `entities` section, or the import is refused (this catches a
  file that was edited by hand inconsistently). The audit trail survives.
- **Without `operations`** (for example, a file produced by another program):
  each entity is written as new changes by the importing member, so the history
  starts at the import.

The importing device acts as the first `owner` member in the file.

## The full backup (ZIP)

A full backup (Settings, "Make a full backup") is one ZIP file:

```text
housewright-export.json   the JSON export described above, unchanged
blobs/<sha256>            the bytes of each stored file, named by their hash
README.txt                a short note for whoever opens the zip
```

Only blobs some live (not deleted) attachment uses are included, thumbnails
too. Photos are JPEG as stored (at most 1600 px on the long edge); PDFs are
byte for byte as added. To check a blob, compute the SHA-256 of its bytes:
it must equal its name. Restoring needs an empty database, checks every
blob first, then imports the JSON exactly as an import does. "Restore" also
accepts a plain JSON export, which brings back everything except file
contents. The code is `crates/core/src/backup.rs`.

## What is not in an export or a backup (Phase 6)

Sync's keys and settings are kept in the device's database (`sync_state`,
D-95) and are **never** written to an export or a full backup: not the
device's key, not the household key chain, not the recovery key (which is
not stored anywhere), not the server address. An export or backup is the
household's data in the clear, readable without any key; keep it as safe as
the sealed volume.

How the two ways of getting data back relate:

| | Full backup (ZIP) | Recovery key |
|---|---|---|
| What it is | A file you keep (a USB stick, a cloud drive) | 36 characters on paper (the sealed volume) |
| Holds | Everything, readable by anyone who has the file | Nothing by itself: it unlocks the household on the sync server |
| Needs | Nothing else | The sync server, still running, and its address |
| Restores to | A fresh device, as a new, unsynced household (turn sync on again to share it) | A fresh device, as another owner device of the same synced household |
| Up to date | As of the day it was made | As of the last sync of any device |

A backup restored on a fresh device starts a new household history on that
device as far as sync is concerned: its operations are pushed as that
device's own when sync is set up on it. Restoring a backup into the same
synced household (merging it back) is not supported.

The downloaded US recall list (`recall_cache`, D-141) is public data kept
on one device only: it is not in an export or a backup, is not synced, and
is downloaded again when needed.

## Compatibility

New optional fields may appear in any version; readers should ignore fields
they do not know. Anything that changes the meaning of an existing field,
removes one, or adds a new kind bumps `version`.

- **Version 2** (Phase 3) added the `profile` and `suggestion_answer` kinds and
  the optional `hard_freeze` in a property's climate. Version 1 files import
  unchanged (tested with `crates/core/tests/fixtures/export-v1-phase2.json`,
  written by the Phase 2 code). A version 1 reader refuses version 2 files
  with a clear message rather than misreading them.
- **Version 3** (Phase 4) added the `completion_detail` and `template_fields`
  kinds and optional fields on `household`, `attachment` and `fact`.
  Version 1 and 2 files import unchanged (tested with
  `crates/core/tests/fixtures/export-v2-phase3.json`, written by the Phase 3
  code).
- **Version 4** (Phase 5) added the `binder_print` kind and the optional
  `emergency` field on `contractor`. Version 1, 2 and 3 files import
  unchanged (the Phase 3 fixture is also printed as a binder in
  `crates/api/tests/binder.rs`).
- **Version 5** (Phase 7) added the `triggered` schedule kind, `kind` on
  `member` and `done_by` on `completion` (Home Assistant, D-118, D-122).
  Version 1 to 4 files import unchanged. A version 4 program refuses a
  version 5 file; it would also fail to read a triggered task arriving by
  sync, so every device in a household should run the Phase 7 version.
- **Version 6** (Phase 8) added the floor plan kinds `wall`, `opening`,
  `plan_pin` and `plan_label`, and optional fields `household.units`,
  `floor.background`, `room.outline` and `fact.pin` (D-130). Version 1 to
  5 files import unchanged (the Phase 2 and Phase 3 fixtures still import
  in the core's tests). A version 5 program refuses a version 6 file, and
  it cannot apply plan operations arriving by sync, so every device in a
  household should run the Phase 8 version before anyone draws a plan.
- **Version 7** (Phase 9) added the `recall_dismissal` kind and optional
  fields: `item.price`, `item.value`, `warranty.months`,
  `warranty.register_url`, `contractor.company`, `.website`,
  `.service_area`, `.would_hire_again`, `.private_notes`, and
  `completion_detail.contractor` and `.contractor_cleared` (D-146).
  Version 1 to 6 files import unchanged (the Phase 2 and Phase 3 fixtures
  still import in the core's tests, and a version 6 export of a Phase 8
  household imports in the API's tests). A version 6 program refuses a
  version 7 file. Over sync it cannot apply a `recall_dismissal`, and when
  it saves a thing or a contractor it does not know the new fields, so its
  save clears them (a store writes only the fields it knows): every device
  in a household should run the Phase 9 version.

