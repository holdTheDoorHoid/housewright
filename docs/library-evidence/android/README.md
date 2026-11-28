# Library hookup on the Android emulator

2026-11-28, x86_64 debug APK (`cargo tauri android build --debug --target
x86_64 --apk`), Android 15 emulator `hw_api35`, installed with `install -r`
over the Phase 9 household (no storage migration in this change; the
existing things and tasks were all there).

| File | Check | Result |
|---|---|---|
| `01-opened.png` | The upgraded app opens on the old data | Pass |
| `02-typed-suggestion.png` | Typing make "Midea" and model "MAD20S1QWT" on a new dehumidifier suggests the library's family, with its reasons | Pass: "Same brand: is it one of these?" with "Midea Cube and MAD series dehumidifiers" (that family lists model examples but no model patterns, so it is offered by name) |
| `03-family-chosen.png` | "This one" links it before adding; nothing applied until the tap | Pass |
| `04-thing-from-midea.png` | The thing's page shows "From Midea": the manual PDF link, "Save the official manual to this thing", support, the CC BY credit | Pass |
| `05-saving.png`, `06-manual-saved.png` | "Save the official manual to this thing" downloads the **real** PDF from `www.midea.com` (the family's `manual-pdf` link) onto the phone | Pass: "Saved the manual with this thing. It stays on your devices."; the file appears under Photos and documents as "Midea manual - MAD20S1QWT-2024-Cube-Manual.pdf" |
| (not committed) | The saved manual opens in the phone's PDF viewer | Pass: top activity `com.google.android.apps.docs/com.google.android.apps.viewer.PdfViewerActivity`, the Midea user manual's cover page shown. The screenshot is not in the repository because it shows the manufacturer's manual (D-127). |

Found on the way and fixed afterwards in the source (the APK tested here
predates both): on a phone-width screen the "Save the official manual"
button overlapped the manual's link (`04`); the button now sits under the
link. The brand-only reason said the model "does not fit a model pattern
the library knows", which reads wrongly when a family lists no patterns;
it now says the library does not know this model number yet and to pick by
name.

The emulator was shut down with `emu kill`. The Android build directories
were deleted except the final APK
(`app/src-tauri/gen/android/app/build/outputs/apk/universal/debug/`, not
committed).
