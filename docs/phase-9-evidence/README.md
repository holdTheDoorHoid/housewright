# Phase 9 evidence

What was looked at for contractors, warranties, recall checks and the
insurance inventory (2026-11-28). Everything here was made by the tests,
from a test household; nothing is a real home.

## Web (Playwright, production web build in Chrome)

Made by `app/tests/phase9.spec.ts`:

| File | What it shows |
|---|---|
| `contractor-page.png` | A contractor's page right after adding them: Call and Email, licence "Ends soon", the private rating and notes under "Just for you · Private". |
| `warranties.png` | The Warranties view with one thing in each state: ending soon (with the check-up suggestion and the registration button), covered, ended. |
| `recall-match.png` | A recall check against the recorded CPSC response (the network intercepted): one match, why it matched, hazard, what to do, "Read the official notice" and "Not mine". |
| `insurance.png` | The insurance inventory screen with "Include sensitive items" ticked. |
| `insurance-inventory.pdf` | The inventory PDF the browser made (Typst in WebAssembly), with the private thing left out. |
| `contractor-phone.png` | A contractor's page at phone width (390 px), with no sideways scrolling. |

## The inventory PDF, page by page

`insurance-pages.png` is the native inventory of the binder tests' demo
household (`HW_BINDER_OUT=… cargo test -p housewright-api --test binder
insurance`), rendered with `pdftoppm`. The first version gave every room
its own page and every thing a box, even with nothing recorded about it
(nine mostly empty pages for twenty things). Fixed: rooms follow one
another, things with nothing recorded are named in one line per room,
receipts share a page with the totals, and the cover no longer repeats its
title. Four pages now.

## Android

See `android/README.md` (or the note there on why it was skipped).
