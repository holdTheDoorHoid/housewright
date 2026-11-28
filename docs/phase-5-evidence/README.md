# Phase 5 evidence: the house binder

Pages from the PDFs that `crates/api/tests/binder.rs` makes for its demo
household ("14 Larch Lane": onboarding answers, every task switched on,
sixteen facts of which five are sensitive, a whole thing marked
sensitive, two members, three contractors with private ratings, ten
completed jobs with costs, quirks, photos). Rendered with
`pdftoppm -r 66`. To make them again:

```sh
HW_BINDER_OUT=/tmp/binder cargo test -p housewright-api --test binder
```

| File | What it shows | What was checked by eye |
|---|---|---|
| `emergency-page.png` | Page 1 of every main volume, and the emergency edition on its own (Letter). | Shutoffs and who to call in large type; the two shutoff photos with captions; four action boxes; footer "Printed … · Emergency 1". |
| `emergency-page-crowded.png` | The same page with six more long shutoffs and six emergency contractors. | Still one page: long entries are cut with "…" and the type steps down. Readable, but small: the screen's advice is to keep page 1 short. |
| `full-about.png` | About this binder: how to use it, where the sealed volume is kept, contents. | |
| `full-water-1.png` | A section of things: each thing, then each switched-on task with how often, last and next, who, why with its source, and steps. | Per-section page numbers ("Water 1"…"Water 6"). |
| `full-calendar-1.png` | The next 12 months: monthly jobs listed once, then month by month in two columns. | Was six pages of table rows at first; now two pages. |
| `sealed-cover.png`, `sealed-codes.png` | The sealed volume: "SEALED — keep in a safe place", then codes and access. | The cover first said "SEALED" twice; fixed. |
| `succession-sealed-accounts.png` | The succession edition's sealed "Accounts to transfer" tick list. | |
| `handover-history.png` | Handover edition: history with "Previous owner", the contractor named, no costs. | |
| `reprint-changed-facts.png` | "Print the changed sections" after one fact was added: only Facts, numbered "Facts 1". | |

Layout problems found by looking and fixed: label/value pairs printed
their first row in bold (they were a table with a header style; now a
grid); photo captions did not line up under centred photos; the
emergency page could run to two pages for a busy household (now shrinks
to fit, tested); the 12-month calendar was six pages; the sealed cover
repeated its title. Poppler prints "Syntax Error: Suspects object is
wrong type (boolean)" for Typst's tagged PDFs; the files open and print
normally, and Chrome's viewer shows no error.

## Android emulator

Android 15 emulator (`hw_api35`, x86_64), driven only through
`~/Android/emu-adb`, 2026-11-28. The x86_64 debug APK was installed with
`install -r` over the earlier build, so the existing household (schema 3)
was migrated to schema 4 in place; its Today list was intact
(`android/01-launch-after-upgrade.png`; the "System UI isn't responding"
dialog on that first screenshot is the emulator's own, from booting
headless).

| Check | Result | Evidence |
|---|---|---|
| House binder screen from Home | **Pass.** Editions, section list, paper, print buttons, where the sealed volume is kept, who to call. | `android/02-binder-screen.png`, `android/03-binder-sections.png`, `android/04-sealed-place-saved.png` |
| Main binder opens in the phone's PDF viewer | **Pass.** About 2 s from the tap to the Google PDF viewer for this household (typeset in-process, debug build). Page 1 is the emergency page; the about page shows where the sealed volume is kept. | `android/05-main-binder-in-viewer.png` |
| Change markers | **Found a bug, fixed.** Straight after the first print, "Maintenance history" showed as changed: the screen printed the household binder without costs while the status compared with costs. The screen now sends the handover choices only for the handover edition, and the status compares with the same choices (regression test `handover_change_markers_follow_the_chosen_options`). After the fix, the stale mark from the first print was cleared by "Print the 1 changed section", which printed only History ("History 1"). | `android/06-changed-markers.png`, `android/07-reprint-changed-section-viewer.png`, `android/08-after-reprint-main-up-to-date.png` |
| Sealed volume as its own PDF | **Pass.** `house-binder-full-SEALED.pdf`, titled "SEALED — keep in a safe place", with the recovery key placeholder page. | `android/09-sealed-volume-viewer.png` |
| Up to date after printing both | **Pass.** "Nothing has changed since you printed it on Nov 28." | `android/10-everything-up-to-date.png` |

Not checked on the emulator: the share sheet for the binder (the same
plugin path as the Phase 4 backup), printing to a real printer, and a
real phone.
