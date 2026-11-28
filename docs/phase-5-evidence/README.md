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

ANDROID_SECTION
