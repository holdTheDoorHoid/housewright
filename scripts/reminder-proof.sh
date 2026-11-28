#!/usr/bin/env bash
# Phase 2 reminder proof on the Android emulator, step by step.
#
# For the OWNER to run by hand. It pauses at each step and says what to look
# for. Evidence (dumpsys excerpts, screenshots) goes in docs/phase-2-evidence/.
#
# Shared-lab rules (see docs/DEVELOPING.md):
#   - an isolated adb server on port 5038, never the default port;
#   - the emulator only, always addressed as -s emulator-5554;
#   - never `adb kill-server`.
#
# Before you start:
#   1. Build the debug APK:  (cd app && cargo tauri android build --debug --target x86_64 --apk)
#   2. Start the emulator:   $ANDROID_HOME/emulator/emulator -avd hw_api35 -port 5554 \
#                              -no-window -no-audio -gpu swiftshader_indirect -no-boot-anim &
#      (create the AVD once with the avdmanager line in docs/DEVELOPING.md)
set -euo pipefail

export ANDROID_ADB_SERVER_PORT=5038
: "${ANDROID_HOME:=$HOME/Android/Sdk}"
ADB="$ANDROID_HOME/platform-tools/adb -s emulator-5554"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
EV="$ROOT/docs/phase-2-evidence"
APK="$ROOT/app/src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk"
PKG=app.housewright
mkdir -p "$EV"

pause() { printf '\n>>> %s\n    Press Enter when done. ' "$1"; read -r _; }
shot() { $ADB exec-out screencap -p > "$EV/$1.png"; echo "    saved $EV/$1.png"; }
notes() { $ADB shell dumpsys notification --noredact | grep -A12 "pkg=$PKG" > "$EV/$1.txt" || true; echo "    saved $EV/$1.txt"; }
alarms() { $ADB shell dumpsys alarm | grep -B2 -A8 "$PKG" > "$EV/$1.txt" || true; echo "    saved $EV/$1.txt"; }

echo "== 0. Install and set up"
$ADB wait-for-device
until [ "$($ADB shell getprop sys.boot_completed | tr -d '\r')" = 1 ]; do sleep 2; done
[ -f "$APK" ] || APK=$(ls "$ROOT"/app/src-tauri/gen/android/app/build/outputs/apk/*/debug/*.apk | head -1)
$ADB install -r "$APK"
$ADB shell pm grant $PKG android.permission.POST_NOTIFICATIONS
$ADB shell monkey -p $PKG -c android.intent.category.LAUNCHER 1 >/dev/null
pause "In the app: enter a name, tap Start, Add a thing -> Smoke alarm -> Add -> 'Switch on the top 3'.
    Then open Settings and check it says 'Notifications are allowed.'"
shot 00-app-ready

echo "== (a) A reminder fires while the app is in the background"
pause "In Settings tap 'Try a reminder now' (it fires in about 30 s), then come back here."
$ADB shell input keyevent KEYCODE_HOME
alarms a-alarm-scheduled
echo "    LOOK FOR: an alarm for $PKG with action app.housewright.reminders.FIRE"
sleep 45
notes a-notification
$ADB shell cmd statusbar expand-notifications; sleep 2; shot a-notification-shade
$ADB shell cmd statusbar collapse
echo "    LOOK FOR: a notification from $PKG, title = a smoke alarm task,"
echo "    with actions 'Done' and 'Reschedule' (dumpsys shows actions=2)."

echo "== (b) A reminder fires after the process is killed (am kill)"
$ADB shell monkey -p $PKG -c android.intent.category.LAUNCHER 1 >/dev/null
pause "In Settings tap 'Try a reminder now' again, then come back here."
$ADB shell input keyevent KEYCODE_HOME; sleep 2
$ADB shell am kill $PKG
sleep 1
echo "    process after am kill: '$($ADB shell pidof $PKG | tr -d '\r')' (should be empty)"
sleep 45
notes b-notification-after-kill
$ADB shell cmd statusbar expand-notifications; sleep 2; shot b-notification-after-kill
echo "    LOOK FOR: a new notification although the process had been killed."

echo "== (c) Done in the notification records a completion without opening the app"
TITLE_LINE=$(grep -m1 'android.title=' "$EV/b-notification-after-kill.txt" || true)
echo "    notification title: $TITLE_LINE"
$ADB shell am kill $PKG
pause "In the open notification shade on the emulator, tap 'Done' (use the screenshot to find it:
    $ADB shell input tap X Y). Do NOT open the app."
sleep 3
$ADB logcat -d -s housewright:I | tail -20 > "$EV/c-logcat.txt"; echo "    saved $EV/c-logcat.txt"
echo "    LOOK FOR in c-logcat.txt: 'done recorded from notification for <task id>'"
echo "    and 'scheduled N reminders'. The notification should be gone:"
notes c-after-done
$ADB shell cmd statusbar collapse
$ADB shell monkey -p $PKG -c android.intent.category.LAUNCHER 1 >/dev/null
pause "Now open History in the app."
shot c-history
echo "    LOOK FOR: \"<your name> marked '<task>' done\" at the time you tapped Done."

echo "== (d) Reminders survive a reboot"
pause "In Settings tap 'Try a reminder now' once more."
alarms d-alarms-before-reboot
$ADB reboot
$ADB wait-for-device
until [ "$($ADB shell getprop sys.boot_completed | tr -d '\r')" = 1 ]; do sleep 2; done
sleep 10
$ADB logcat -d -s housewright:I | tail -20 > "$EV/d-logcat-after-boot.txt"
alarms d-alarms-after-boot
echo "    LOOK FOR: 'rescheduling after android.intent.action.BOOT_COMPLETED' in d-logcat-after-boot.txt,"
echo "    and FIRE alarms for $PKG again in d-alarms-after-boot.txt."
echo "    NOTE: the test reminder is not part of the plan, so if it was due during the reboot"
echo "    it is lost; the planned reminders (due dates) are what must come back."

echo "== force-stop (for the record, expected to FAIL by Android design)"
pause "Optional: tap 'Try a reminder now', then press Enter to force-stop."
$ADB shell am force-stop $PKG
sleep 45
notes force-stop
echo "    EXPECTED: no notification. Android cancels alarms of force-stopped apps"
echo "    until the person opens the app again (see D-62)."

echo "Done. Shut the emulator down with: $ADB emu kill"
