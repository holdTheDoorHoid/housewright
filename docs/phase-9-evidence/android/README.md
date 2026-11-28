# Phase 9 on the Android emulator

2026-11-28, x86_64 debug APK (`cargo tauri android build --debug --target
x86_64 --apk`), Android 15 emulator `hw_api35`, installed with `install -r`
over the Phase 8 household (schema 6 → 7 migrated on first open; the
existing things, tasks and plan were all there).

| File | Check | Result |
|---|---|---|
| `01-opened.png` | The upgraded app opens on the old data | Pass (the "System UI isn't responding" dialog is the emulator's own, from a slow first boot) |
| `02-contractor-page.png` | A contractor added on the phone | Pass |
| `03-call-opens-dialer.png` | "Call" opens the phone's dialer with the number filled in, nothing dialled | Pass: top activity `com.google.android.dialer/.main.impl.MainActivity`, number "(215) 555-0101" |
| `04-recalls-opt-in.png` | Recall checks are off until switched on, with the plain promise | Pass |
| `05-recalls-downloading.png`, `06-recalls-live-result.png` | Switching on downloads the **live** CPSC list on the emulator | Pass: 3,156 recalls (ten years, ten yearly requests) in about 30 seconds; "No recalls found for your things" (this household has no make or model recorded). The app's content security policy allowed the one CPSC origin. |
| `07-warranty-notice.png` | A warranty reminder fires | Pass: a warranty ending Dec 6 (set with the date picker) gave a notice dated Nov 29 09:00. The emulator clock was set to Nov 29 08:59 (`date`, as root; `TIME_SET` re-planned the reminders), and at 09:00:44 the notification "Tub and shower caulk: warranty ends in a week" appeared, with the check-up advice and **no** Done or Reschedule buttons. |
| `08-notice-opens-thing.png` | Tapping the notice opens the thing | Shown, but weak evidence: the app was already on that thing's page when the notice was tapped. |
| `09-inventory-pdf.png` | The insurance inventory typeset in-process opens in the phone's PDF viewer | Pass |

Found and fixed on the way:

- The Kotlin notice code declared `open` twice (the Gradle build failed);
  renamed.
- The emulator's app had lost its notification permission, and the notice
  path returned silently when notifications were not allowed (the first
  attempt fired at 09:00 and showed nothing, with nothing in the log).
  Granted with `pm grant` for the test. The interface asks for the
  permission in Settings as before (D-62).
- Reminders were re-planned only when the app started, so a warranty or
  licence date changed in the app was not scheduled until the next start.
  The edit forms now ask for a re-plan after saving (in the source; the
  APK tested here predates that change, so the test restarted the app).

Afterwards the emulator's automatic time was switched back on, adbd was
returned to non-root, and the emulator was shut down with `emu kill`.
The Android build directories were deleted except the final APK
(`app/src-tauri/gen/android/app/build/outputs/apk/universal/debug/`, not
committed).
