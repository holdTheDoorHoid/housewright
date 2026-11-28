# Phase 2 Android reminder proof — 2026-11-27

Device: Android emulator, AVD `hw_api35` (Android 15, x86_64, google_apis),
debug APK from branch `phase-2-app-shell`. Driven through `~/Android/emu-adb`
(emulator-only adb helper, isolated server on port 5038).

| Check | Result | Evidence |
|---|---|---|
| Setup: first run, add a smoke alarm, switch on the top 3 tasks | pass | `00`–`05` screenshots |
| (a) Reminder fires while the app is in the background | **pass**: shown ~52 s after scheduling (inexact alarm), title, reason, Done + Reschedule | `a-alarm-scheduled.txt`, `a-notification-shade.png` |
| (b) Reminder fires after `am kill` | **pass**: no process after kill; Android started a new process at 19:59:50 and showed the reminder | `b-logcat.txt`, `b-notification-after-kill.txt` |
| (c) Done in the notification, app not opened, process killed first | **pass**: "done recorded from notification", plan re-made, notification cleared, launcher stayed in front; History shows "Alex marked … done" at 8:00 PM | `c-before-done.png`, `c-logcat.txt`, `c-history.png` |
| (d) Reminders survive a reboot | **pass**: "rescheduling after BOOT_COMPLETED"; the same two alarms (Nov 29 and Dec 4, 09:00) before and after | `d-alarms-before-reboot.txt`, `d-alarms-after-boot.txt`, `d-logcat-after-boot.txt` |
| Force-stop | not run: by Android design, alarms are cancelled until the app is opened (D-62) | — |

## Bug found and fixed
The "Try a reminder now" test notification was attached to a real task, so
tapping Done on it logged a real completion (in (c) this marked the ten-year
smoke-alarm replacement as done). Fixed: test reminders are titled "Test: …"
and their Done records nothing. Re-verified: `e-test-reminder.png`,
`e-logcat.txt` ("test reminder done … nothing recorded"), `e-history.png` (no
new completion after 8:00 PM).

## Not covered
- Real phones with aggressive battery management (Samsung, Xiaomi, OnePlus).
- Reminder timing over days (only "fires within about a minute" and "planned
  alarms survive reboot" were observed).
