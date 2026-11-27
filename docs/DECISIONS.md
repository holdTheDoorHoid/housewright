# Decision log

This file is the source of truth for what Housewright is and why. When a decision
changes, add a new entry that supersedes the old one; do not edit history.

- **Owner** = decided by the project owner in the founding interview (2026-11-27).
- **Proposed** = proposed by the design agent, not yet confirmed by the owner.
  Proposed entries can be changed freely until confirmed.

## Product

**D-01 Purpose (Owner).** An open source app that reminds people of, and keeps
records of, home maintenance: recurring tasks, an inventory of the things in the
house with their manuals and warranties, who did what and when, and the
contractors who help. It should take someone who knows nothing about their house
and help them keep track, and scale up to landlords with several houses or large
households with several people.

**D-02 Name (Owner).** Housewright (an old word for a builder of houses). A
formal trademark search is still to do before any store listing (see Q-01).

**D-03 First user (Owner).** The first release is built for an *engaged
household*: people who already look after their house and want it organised
(inventory, manuals, logs, several family members sharing). Novice guidance and
landlord features are designed into the data model from day one but are polished
later.

**D-04 Platforms (Owner).** Android, iOS and a web page. Android and web ship
first; iOS follows once an Apple developer account and Mac build path are sorted.

**D-05 License (Owner).** AGPL-3.0 for all code. Anyone can use and self-host it;
anyone who runs a modified copy as a service must publish their changes. This
protects the paid hosted option from closed competitors.

**D-06 Region (Owner).** US first (US recall data, US climate zones, imperial
units by default, English), but units, languages, hemisphere and climate data are
designed in from the start so other countries are additions, not rewrites.

**D-07 Public repository (Owner).** Public on GitHub from day one.

## Architecture

**D-10 Stack (Owner).** A Rust core does the real work (data model, schedules,
storage, sync, encryption). The user interface is one Svelte web interface shared
by Android, iOS and desktop through Tauri 2, and by the web page through the same
core compiled to WebAssembly. The server is Rust.

**D-11 Local-first (Owner).** Every device holds a full copy and works offline.
The app works on a single device with no account and no server. Syncing is
optional: to a self-hosted server, or to the paid hosted server.

**D-12 Merge model (Owner).** Edits made offline on different devices merge
automatically when they reconnect. Completion records are append-only, so they
never conflict.

**D-13 Sync mechanism (Proposed).** An operation log: every change is a small
signed operation stamped with a hybrid logical clock. Ordinary fields merge as
"last writer wins" per field; completions, notes, photos and audit entries are
append-only sets; deletes are tombstones. The server only stores and relays
encrypted operations and never needs to understand them. Chosen over a full
CRDT document library (Automerge, Loro) because per-field last-writer-wins is
easy to explain to users, keeps SQLite as the real database, and encrypts
naturally one operation at a time. Revisit if rich collaborative text editing
becomes a need.

**D-14 Encryption (Owner).** Data on the hosted server is end-to-end encrypted:
the server stores only data it cannot read.

**D-15 Encryption design (Proposed).** Each household has a random household key.
Each member device has its own key pair; the household key is wrapped to each
member's public key. A printable recovery key is generated at setup and belongs in
the binder's sealed volume (D-41). Operations and files are encrypted with
XChaCha20-Poly1305. Losing every device *and* the recovery key means the data is
gone; the app must say so plainly at setup.

**D-16 Paid extras under encryption (Owner).** Everything stays encrypted. A paid
extra that needs to see data (email/SMS reminders, recall alerts, AI label reading)
is switched on per household, and the app shares only what that extra needs, for
example a task title and due date for email reminders, or the make/model list for
recall checks. The app shows exactly what is shared. Share links for contractors
and tenants carry their own key in the link, so the server still cannot read them.

**D-17 Storage (Proposed).** SQLite everywhere: native SQLite on Android, iOS and
desktop; SQLite compiled to WebAssembly with browser storage (OPFS) on the web.
Photos and documents are stored as encrypted files beside the database, referenced
by content hash.

**D-18 Reminders without a server (Proposed).** Reminders are scheduled as local
notifications on each device, so they work with no server at all. Server push is
an optional extra for the hosted tier.

## Features

**D-20 Schedules (Owner).** Tasks support all four schedule kinds:
- *fixed calendar* (every 3 months on the 1st; every October);
- *floating* (90 days after it was last done);
- *seasonal / climate-aware* ("before first frost", adjusted by zip code and
  hemisphere);
- *usage / sensor based* (after N hours of runtime or N loads, or on a sensor
  event; manual counter entry where there is no Home Assistant).
One-off tasks are also supported.

**D-21 Templates (Owner).** An open community library of maintenance templates:
generic ("washing machine") and make/model specific ("LG WM4000HWA"), as plain
text files anyone can propose by pull request, reviewed before merging. Apps ship
a copy and update it. The generic starting set is written from public guidance.
No scraped manufacturer content; manuals are linked or attached by the user, never
redistributed.

**D-22 Custom fields (Owner).** Templates define fields for items (filter size,
capacity) and for completing a task (pressure reading, parts used, pass/fail).
Users can add their own fields to any template or a single item. Field types:
text, number with unit, date, yes/no, choice, photo.

**D-23 Roles (Owner).** Owner/admin; household member; limited (sees only tasks
assigned to them); and contractor/tenant links (no app needed: view one task or
report a problem).

**D-24 Onboarding (Owner).** A short core questionnaire (about 10 questions: home
type, age, heating, water, climate/zip, who lives there) gives a starter plan in
about 3 minutes. Optional room-by-room walks follow over the next weeks,
photographing labels as you go.

**D-25 Floor plans (Owner).** Full measured floor plans: walls with real
dimensions, angles, windows and doors, area calculations, several floors, item
pins, export to PDF. Built in stages: rooms and pins first, then measurements
(see ROADMAP). Phone scanning (LiDAR) is a later stretch goal.

**D-26 Home Assistant (Owner).** All four:
1. tasks appear in Home Assistant (to-do and calendar entities, due/overdue sensors);
2. Home Assistant data triggers tasks (runtime hours, leak sensors, low batteries);
3. import devices (make, model, room) from Home Assistant;
4. the sync server can be installed as a Home Assistant add-on.

**D-27 Record-keeping (Owner).** Costs and spending per task, item and house;
warranty-expiry and product-recall alerts (US CPSC data); an insurance inventory
export (PDF/spreadsheet with photos, serials, values, receipts); a home facts
sheet (filter sizes, paint colours, bulbs, shutoffs, breakers, codes).

**D-28 Contractors (Owner).** Contacts and trades; job history and costs linked
to items and rooms; licence and insurance expiry tracking; a private rating
that is never shared.

**D-29 Label scanning (Owner).** Photograph an appliance sticker and the app reads
brand, model and serial on the device (free, private, offline) and matches a
template. On the paid tier, an opt-in AI reads messy labels and drafts a
maintenance plan from the manual for the user to review.

**D-30 QR labels (Owner).** Printable QR sticker sheets for appliances. Scanning
opens the item's manual and history and logs a task in one tap. Each sticker also
shows a short human-readable code that matches the binder page. NFC tags later.

**D-31 Paid tier (Owner).** Every app feature is free, including self-hosted sync.
Paying buys hosted sync, backups and storage, plus server-run extras (email/SMS
reminders, recall alerts, AI label reading) under D-16. Pricing is per household,
not per property or per seat (see Q-03).

**D-32 Research data (Owner).** Opt-in, anonymous, off by default: counts and
timings only ("reminder sent; task done 4 days later"), never item names,
addresses or photos. Results are published. Its purpose is to find better reminder
defaults, which the research says nobody knows (see DESIGN-PRINCIPLES P12).

## The house binder

**D-40 House binder (Owner).** The app produces a printable house manual to keep
in a binder in the house. Its purpose is to preserve knowledge if someone dies
suddenly, the house changes hands, or the person who knew is not there. It must
stand on its own without the app.

**D-41 Two volumes (Owner).** The main binder holds everything about running the
house and is safe to leave on a shelf. A separate *sealed volume* holds codes,
account details, the recovery key (D-15) and where valuables are kept; it is
printed separately for a safe, sealed envelope or an estate lawyer, and the main
binder has a page saying where it is. Every field is marked normal or sensitive,
and the app files it into the right volume.

**D-42 Binder editions (Owner).** All four:
- an *emergency first page*: water, gas and electric shutoffs with plan snippets
  and photos, breaker map, who to call;
- a *sale/handover edition* for a buyer or new tenant: systems, ages, service
  history, manuals, transferable warranties, with personal items left out;
- a *succession/estate edition* for someone inheriting the house: contractors,
  what's due next, accounts and utilities to transfer, quirks only you know;
- *stays current*: pages carry a printed-on date; the app tracks what changed
  since the last print and asks you to reprint just the changed pages.

**D-43 Binder rendering (Proposed).** Generate the binder with Typst, a Rust
typesetting engine (Apache-2.0) embedded in the core, so it renders identically
and offline on every platform. Loose-leaf layout: each section starts on its own
page, and pages are numbered by section (e.g. "Water 3") so single sections can be
reprinted.

## Open questions for the owner

- **Q-01** Formal trademark check of "Housewright" (USPTO) before any app store
  listing.
- **Q-02** Template library license. Proposed: CC BY 4.0 for template text (lets
  others reuse it, with credit), separate from the AGPL code.
- **Q-03** Paid tier price and storage limits.
- **Q-04** Who is the publisher on the app stores (personal name or an entity)?
- **Q-05** Should the template library live in its own repository from the start,
  or in this one until it has contributors? Proposed: in this repo under
  `templates/` until v0.1, then split.
