# Phase 4 evidence: Android emulator

Android 15 emulator (`hw_api35`, x86_64), driven only through the
emulator-only helper `~/Android/emu-adb`, 2026-11-27. The x86_64 debug APK
from `cargo tauri android build --debug --target x86_64 --apk` was
installed with `install -r` over the Phase 3 build, so the existing
household (schema 2) was migrated to schema 3 in place: everything was
still there (`01-launch.png`).

| Check | Result | Evidence |
|---|---|---|
| Sticker link: `am start -a android.intent.action.VIEW -d 'housewright://item/<sump pump id>'` with the app in the background | **Pass.** The app came to the front on the Sump pump, with its sticker code and its next task and a Done button. The plugin logged `sticker link for item 01a21ff5-...`. | `02-deep-link-sump-pump.png`, `c-deep-link-am-start.txt` |
| One-tap Done from the sticker screen, then "Add details" | **Pass.** One tap recorded it; the toast offered Add details and Undo. Not due any more, the button turns into a quieter "Done now". | `03-done-from-sticker.png`, `04-add-details.png` |
| Photo in the WebView (file input) | **Pass, after a fix.** The file input opens Android 15's system photo picker ("This app can only access the photos you select"); the pushed test photo was chosen; it was shrunk, stored, and shown as a thumbnail. The first build failed with "expected a file": Android's WebView cannot pass a raw request body to the app, so file bytes now cross the bridge as base64 (D-79). | `05-photo-picker.png`, `06-completion-photo-added.png` |
| Cost after Done | **Pass.** $18.50 parts saved as a completion detail; History says "added details to a completion", with no amount. | `07-cost-saved.png`, `15-history.png` |
| Label reading on a test photo (synthetic rating plate pushed to the gallery) | **Pass.** Make "Rheem", model "XG40TO6EC36U1" (OCR read the 0 as O), serial "0081912345" (Q read as 0), shown for checking before anything is saved. About 24 s from picking the photo to the result in this debug build on the emulator (first use, including loading the models). | `08-label-picker.png`, `09-label-read.png` |
| A PDF opens in the phone's viewer | **Pass, after a fix.** The sticker sheet PDF opened in the Google PDF viewer through the FileProvider. The first try failed because Tauri calls Kotlin commands by camelCase name (`viewFile`), and the error showed as "[object Object]"; both fixed. A PDF attachment opens through the same path. | `11-sticker-page.png`, `12-pdf-open.png` |
| Facts sheet: a sensitive value hidden, then revealed | **Pass.** "Gate code" in Access was sensitive by default, shown as dots with "Tap to reveal", then revealed. History shows "added fact 'Gate code'" and never the value. | `10-home-facts.png`, `13a-fact-form.png`, `13-fact-hidden.png`, `14-fact-revealed.png`, `15-history.png` |

Not checked on the emulator: the in-app "Scan a sticker" from a photo (the
same Rust decoder is exercised by the Rust tests and by Playwright in
Chrome), the phone camera app scanning a printed sticker (the emulator has
no real camera; the link it would open was tested with `am start`), the
full backup's share sheet, and a real phone.

APK size (x86_64 debug, unstripped): 335 MB, almost all debug symbols. The
native library stripped is 48.1 MB, of which the OCR models are 12.2 MB and
the code added by Phase 4's libraries about 5.8 MB in this unoptimised
build (rten 4.2 MB, zip 0.65 MB, image 0.47 MB, QR and PDF 0.34 MB, ocrs
0.11 MB, measured with `llvm-nm --size-sort`). A Phase 3 APK was not
rebuilt for comparison (disk space); the Phase 4 addition is therefore an
estimate of about 18 MB uncompressed in the native library, under the
25 MB limit. A release build (`opt-level = "s"`, stripped) will be smaller
on the code side; the 12.2 MB of models is fixed.

Logs: `c-logcat-housewright.txt` (`logcat -s housewright`).
