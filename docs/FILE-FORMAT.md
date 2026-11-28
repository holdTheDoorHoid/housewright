# Housewright export format (version 3)

Housewright can write everything about a household to one JSON file and read it
back without losing anything. The format is documented here so that your data is
never locked in: another program can read it, and a script can produce it.

`hw export FILE` writes it; `hw import FILE` reads it into a new, empty
database. The code is `crates/core/src/export.rs`.

## Top level

```json
{
  "format": "housewright-export",
  "version": 3,
  "exported_at": "2026-11-27T17:00:00Z",
  "household_id": "01a21f65-3c12-72cb-92ba-a9ba73a49591",
  "entities": { "household": [ ... ], "member": [ ... ], "task": [ ... ] },
  "operations": [ ... ]
}
```

| Key | Meaning |
|---|---|
| `format` | Always `"housewright-export"`. |
| `version` | `3` (since Phase 4; `2` in Phase 3, `1` before). A reader must refuse versions newer than it knows. |
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
| `household` | **id**, **name**, **time_zone** (IANA), currency (ISO 4217; absent means USD) |
| `member` | **id**, **name**, **role** (`owner`, `member`, `limited`), contact (wrapped), reminders `{quiet_hours: {start, end}, maintenance_window: {days, start, end}}` (times as `"HH:MM:SS"`, days as weekday names; D-59) |
| `property` | **id**, **name**, **time_zone**, **climate** `{hemisphere: north/south, first_frost_doy, last_frost_doy, hard_freeze}`, address (wrapped) |
| `floor` | **id**, **property**, **name**, level |
| `room` | **id**, **property**, **name**, floor |
| `item` | **id**, **property**, **name**, room, parent (item id), category, make, model, serial (wrapped), purchased, installed, warranty `{expires, provider, transferable, reference (wrapped)}`, location_note (wrapped), template, extra_fields, custom, sensitivity |
| `template` | the template, exactly as in `templates/README.md` |
| `task` | **id**, **property**, **title**, **owner** (member id), **priority**, **schedule**, **start** (date), item, room, window, steps, why `{text, source}`, minutes, completion_fields, template `{template, task}`, active, paused, not_applicable |
| `completion` | **id**, **task**, **by**, **at**, notes (wrapped), cost, values, contractor, attachments, voids |
| `snooze` | **id**, **task**, **by**, **at**, **until** (date), reason |
| `meter` | **id**, **key**, **name**, **unit**, item |
| `meter_reading` | **id**, **meter**, **at**, **value**, **by** |
| `contractor` | **id**, **name**, trades, phone, email, licence and insurance `{number (wrapped), expires}`, private_rating (1-5, never shared), notes (wrapped) |
| `attachment` | **id**, **sha256** (hex), **name**, **media_type**, **size**, **kind** (`manual`, `receipt`, `warranty`, `photo`, `other`), **about** `{kind, id}`, sensitivity, thumb_sha256 (hex, photos), width, height (pixels), by (member id), at (instant) |
| `note` | **id**, **about** `{kind, id}`, **by**, **at**, **text** (wrapped) |
| `fact` | **id**, **property**, **category** (`shutoffs`, `sizes`, `finishes`, `access`, `utilities`, `other`), **label**, **value**, **sensitivity**, key (the suggested question it answers, e.g. `water_main`), room |
| `profile` | (version 2) the answers to the home questions, one per property: **id** (the property's id), home_type, built, zip (wrapped), heating `[...]`, cooling `[...]`, water_heater, water_supply, sewer, features `{basement: true, ...}`, smoke_alarms, smoke_power, co_alarms, **people** `{adults, children, older_adults, pets}`. A missing answer means "not sure". Values are the snake_case names in `crates/core/src/profile.rs`. |
| `suggestion_answer` | (version 2) **id**, **property**, **key** (`ask:<template id>` or `task:<task id>`), **answer** (`added`, `not_for_us`, `later`), **by**, **at** |
| `completion_detail` | (version 3) **id**, **completion**, **by**, **at**, costs `[{kind: parts/labour/contractor/other, amount: Money}]`, values (as completion `values`). Added after Done. For one completion, the latest detail that has `costs` gives its costs (an empty list clears them); values are overlaid in order. |
| `template_fields` | (version 3) **id** (a template id), **fields** (field definitions the household added to every thing made from that template) |

`schedule` uses the same shapes as templates, plus two that only a real house
can have: `{"kind": "one_off", "due": "2027-03-01"}` and an `anchor` date on
`fixed`. Field definitions (`extra_fields`, `completion_fields`) use the template
field shape. Custom values (`custom`, `values`) map a field key to:

```json
{ "value": { "type": "choice", "value": "gas" }, "sensitivity": "normal" }
{ "value": { "type": "number", "value": { "value": 50.0, "unit": "gal" } }, "sensitivity": "normal" }
```

where `type` is `text`, `number`, `date`, `yes_no`, `choice` or `photo` (an
attachment id).

Completions, snoozes, meter readings, attachments, notes, suggestion
answers and completion details are records: they are never edited. A completion logged by mistake is cancelled by a later
completion whose `voids` is the mistaken one's id.

Attachments hold only a content hash (and, for photos, the hash of a small
thumbnail). The JSON export does not contain file contents; the full backup
does (below).

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
   suggestion_answer, completion_detail), the
   record is the whole `value`; if two records claim one id, the earliest in
   that order wins.
5. `$deleted` marks the entity deleted forever; later edits do not bring it
   back. Completions, snoozes, suggestion answers and completion details
   cannot be deleted; such operations are ignored. Attachments can be.
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
