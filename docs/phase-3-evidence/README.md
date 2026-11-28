# Phase 3 evidence: home questions on the Android emulator

Checked 2026-11-27 on the Android 15 emulator (`hw_api35`, x86_64), with the
x86_64 debug APK built from branch `phase-3-onboarding`
(`cargo tauri android build --debug --target x86_64 --apk`), installed with
`~/Android/emu-adb install -r` after `pm clear app.housewright`, and driven
with `emu-adb shell input tap/text` and `emu-adb exec-out screencap -p`.

| File | What it shows | Check |
|---|---|---|
| `01-first-run.png` | Welcome screen | Says "about 10 questions … about 3 minutes … every question can be skipped" before asking anything |
| `02-question-1.png` | Question 1 of 10, kind of home | One question per screen, progress bar, Back and Not sure; tabs hidden |
| `03-question-2-alarms.png` | Alarms: 2 or 3 smoke alarms, battery, one CO alarm | Several answers on one screen, Next appears |
| `04-question-5-zip.png` | ZIP 19103 | The offline lookup shows "Average first frost around Nov 17, last frost around Mar 30" |
| `05-question-7-checklist.png` | Checklist: basement, sump pump, gutters, washer, dryer | Toggle buttons fit a phone in two columns |
| `06-question-10-people.png` | Who lives here: counts and pets | Says why it is asked; last step's button is "Show my plan" |
| `07-starting-plan.png`, `08-starting-plan-bottom.png` | Your starting plan | Four tasks switched on: test the CO alarm, test the smoke alarm, book the furnace service, test the sump pump; each with why, source, when and minutes; "Your home, so far: 18 things recorded"; frost dates used; "44 more suggested tasks are switched off", link to see everything; "Go to Today" |
| `09-today.png` | Today | Only the four switched-on tasks, plus one card "This week's suggestion" (clean the gutters after the leaves fall, in season) with Switch it on / Not for us / Later |
| `10-today-after-suggestion.png` | After "Switch it on" | The gutter task joins Today; the card is gone for the week; toast confirms |
| `11-settings-redo.png` | Settings | "Redo the home questions", saying it only adds |
| `12-redo-prefilled.png` | Redo, question 1 | Earlier answer ("A house") is selected |

Saving the answers and building the plan is one database transaction
(D-70); in the web build it took about 2.5 s on the heavily loaded test
machine, down from about 21 s before batching. It was not timed on the
emulator.

Found and fixed during the check: on the starting plan the source ran into
the due date ("…guidance.Due today"). The screenshots show the old text; the
fix (`StartingPlan.svelte`) is in the same branch and was checked in the web
build, not again on the emulator.

Noted, not changed: a seasonal task whose season is already open shows "Was
due Sep 1" on the day it is switched on (the furnace service in late
November). It is accurate (the autumn window opened on Sep 1) but can read
as if something was missed.

The system's "Pixel Launcher / System UI isn't responding" dialogs at the
start were the emulator under heavy host load (load average about 19), not
the app; tapping Wait cleared them.
