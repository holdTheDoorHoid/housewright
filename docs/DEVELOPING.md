# Developing Housewright

## Layout

| Path | What it is |
|---|---|
| `crates/core` | The core: model, schedule engine, reminders plan, storage, op log. |
| `crates/api` | One JSON command surface over the core, used by every front end (D-63). |
| `crates/media` | Photos, QR codes, the sticker PDF and on-device OCR (Phase 4). |
| `models/ocr` | The OCR models (12.2 MB, CC BY-SA 4.0); see its README. |
| `crates/cli` | The `hw` command-line tool. |
| `web/` | The API compiled to WebAssembly, with SQLite on OPFS (D-61). |
| `app/` | The Svelte 5 interface (`src/`), vitest tests, Playwright tests (`tests/`). |
| `app/src-tauri` | Tauri 2 shell for Android (and desktop). Outside the Cargo workspace. |
| `app/src-tauri/plugins/reminders` | Android reminders: alarms, notification buttons, boot receiver (D-62). |
| `tools/climate-build` | Rebuilds `crates/core/data/us-zip-climate.bin`; see [DATA-SOURCES.md](DATA-SOURCES.md). |

## Rust checks

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p housewright-web --target wasm32-unknown-unknown -- -D warnings
cargo test --workspace
```

Do not turn on incremental compilation or set a shared `CARGO_TARGET_DIR`;
`sccache` is the shared cache.

The OCR, image and QR crates are compiled optimised even in debug and test
builds (`[profile.dev.package]` in the workspace `Cargo.toml` and in
`app/src-tauri/Cargo.toml`); unoptimised, reading one label takes minutes.
`cargo test -p housewright-media --test ocr` reads a synthetic rating plate
with the bundled models.

## Web

Needs Rust with the `wasm32-unknown-unknown` target, `wasm-pack`, and Node 22+.

```sh
cd app
npm ci
npm run dev:web        # builds web/pkg, then serves on http://localhost:5173
npm run build:web      # production bundle in app/dist-web (includes ocr/*.onnx)
npm run preview:web    # serves app/dist-web on http://localhost:4173
```

The web build serves the OCR models as `ocr/*.onnx` beside the app (a Vite
plugin in `vite.config.ts` reads them from `models/ocr/`); the worker fetches
them only when a label is first read. The Tauri build does not include them
in its web assets; Android compiles them into the native library instead
(Cargo feature `ocr`; build with `--no-default-features` to leave them out).

Use `localhost`: OPFS needs a secure context. Only one tab can own the
database at a time. A second tab shows "Already open elsewhere".

Interface checks:

```sh
npm run check          # svelte-check (TypeScript), no warnings allowed
npm test               # vitest: Today flow, reschedule/review, formatting, command list
npm run build:web && npx playwright test   # drives the real web build in Chrome
```

Playwright uses the installed Google Chrome (`channel: 'chrome'`). On a machine
without it, run `npx playwright install chromium` and `PW_CHANNEL=chromium`.

Web reminders use the Notifications API, and only while the page is open.
Without a server there is no reliable way to remind someone when the tab is
closed. That is Phase 11 (server push for the hosted tier).

## Android

Needs Java 17, the Android SDK with an NDK, and the Rust Android targets
(`rustup target add x86_64-linux-android aarch64-linux-android`), plus
`cargo install tauri-cli --version "^2" --locked`.

```sh
export ANDROID_HOME=$HOME/Android/Sdk
export NDK_HOME=$ANDROID_HOME/ndk/30.0.16248370
export JAVA_HOME=/usr/lib/jvm/java-17-openjdk-amd64
export ANDROID_ADB_SERVER_PORT=5038      # see "Shared lab" below: always set this

cd app
# Development: x86_64 only (the emulator). One ABI keeps builds small.
cargo tauri android build --debug --target x86_64 --apk
# Release for phones: aarch64 only.
cargo tauri android build --target aarch64 --apk
```

`app/src-tauri/gen/android` is the generated Gradle project. It is committed
because it carries our changes: `compileSdk`/`targetSdk` 36, to match the
installed platform. Re-running `cargo tauri android init` overwrites them.

Live reload on the emulator: `cargo tauri android dev` (it serves Vite on the
host; the emulator reaches it through the Tauri dev proxy).

### Shared lab: the emulator and adb

This machine is a shared hardware lab (see `~/.claude/CLAUDE.md`). For this
project:

- **Every** shell that runs `adb`, `emulator` or `cargo tauri android` must
  `export ANDROID_ADB_SERVER_PORT=5038`. That gives this project its own adb
  server, away from the lab's server on the default port.
- Address only the emulator, by serial: `adb -s emulator-5554 ...`. Never a
  USB device.
- Never run `adb kill-server`, and never call `lab exclusive`.
- Agents: the owner's settings deny plain `adb` to agents. Do not route around
  it. Instead use `~/Android/emu-adb`, an emulator-only helper the owner has
  explicitly allowed: it fixes the port to 5038 and the target to
  `emulator-5554`, and refuses `kill-server`, device-selection flags and
  network/USB commands. Example: `~/Android/emu-adb shell getprop sys.boot_completed`.

Create the emulator once (Android 15, x86_64, KVM):

```sh
$ANDROID_HOME/cmdline-tools/latest/bin/avdmanager create avd -n hw_api35 \
  -k "system-images;android-35;google_apis;x86_64" -d pixel_6
```

Run it headless:

```sh
export ANDROID_ADB_SERVER_PORT=5038
$ANDROID_HOME/emulator/emulator -avd hw_api35 -port 5554 \
  -no-window -no-audio -gpu swiftshader_indirect -no-boot-anim &
$ANDROID_HOME/platform-tools/adb -s emulator-5554 wait-for-device
$ANDROID_HOME/platform-tools/adb -s emulator-5554 install -r \
  app/src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
# ... when finished:
$ANDROID_HOME/platform-tools/adb -s emulator-5554 emu kill
```

### The reminder proof

`scripts/reminder-proof.sh` walks through the four checks Phase 2 must pass,
pausing at each step and saving evidence to `docs/phase-2-evidence/`:

| Step | What it checks | What passing looks like |
|---|---|---|
| (a) | A reminder fires while the app is in the background | `a-notification.txt` shows a notification from `app.housewright` with 2 actions (Done, Reschedule); `a-notification-shade.png` shows it |
| (b) | It fires after `am kill` | `pidof` is empty after the kill, and a notification still appears (`b-*.txt/png`) |
| (c) | Done in the notification records a completion without opening the app | `c-logcat.txt` has `done recorded from notification for <id>`; the notification is gone; `c-history.png` shows "… marked '…' done" |
| (d) | Reminders survive a reboot | `d-logcat-after-boot.txt` has `rescheduling after android.intent.action.BOOT_COMPLETED`; `d-alarms-after-boot.txt` lists FIRE alarms again |
| force-stop | (documented limit) | No notification: Android cancels a force-stopped app's alarms until it is opened again |

The "Try a reminder now" button in Settings schedules a reminder for a real
task 30 seconds ahead, using the same alarm and notification path as planned
reminders. That makes the checks quick.

How it works (D-62): the interface calls `plugin:reminders|sync`. The Kotlin
plugin asks the core for the plan over JNI (`reminder_plan`) and sets inexact
alarms. When an alarm fires, `AlarmReceiver` posts the notification. The Done
button goes to `ActionReceiver`, which calls the core's `complete` command over
JNI and then plans again. `BootReceiver` plans again after a boot, an app
update or a time change. Logs use the tag `housewright`
(`adb -s emulator-5554 logcat -s housewright`).
