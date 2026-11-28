# Developing Housewright

## Layout

| Path | What it is |
|---|---|
| `crates/plan` | Floor plan geometry in whole millimetres (Phase 8, D-130). Its shared test vectors (`tests/vectors.json`) are also checked by `app/src/lib/plan/geometry.test.ts`. |
| `crates/core` | The core: model, schedule engine, reminders plan, storage, op log. |
| `crates/api` | One JSON command surface over the core, used by every front end (D-63). |
| `crates/media` | Photos, QR codes, the sticker PDF and on-device OCR (Phase 4). |
| `crates/binder` | The house binder: document model, redaction, editions, Typst typesetting (Phase 5). Fonts in `crates/binder/fonts/`. |
| `models/ocr` | The OCR models (12.2 MB, CC BY-SA 4.0); see its README. |
| `crates/crypto` | Keys, envelopes, encrypted batches and files, the roster, signed requests, pairing, the recovery key (Phase 6; design in [SECURITY.md](SECURITY.md)). |
| `crates/sync` | The sync client: push, pull, files, pairing, removal, recovery. `native-http` feature: the HTTP client for Android, desktop and tests. |
| `crates/server` | `housewright-server`, the ciphertext-only relay (axum + SQLite); [SELF-HOSTING.md](SELF-HOSTING.md). |
| `crates/cli` | The `hw` command-line tool. |
| `crates/bridge` | `housewright-bridge`, the Home Assistant bridge: a headless household device with a token-protected local API (Phase 7, D-117). |
| `integrations/home-assistant` | The Home Assistant integration (Python, the one exception to the language rule, D-117), its tests, blueprints and the add-on. |
| `web/` | The API compiled to WebAssembly, with SQLite on OPFS (D-61). |
| `web-binder/` | The binder's typesetter (Typst) as a second WebAssembly module, loaded only when someone prints (D-80). |
| `app/` | The Svelte 5 interface (`src/`), vitest tests, Playwright tests (`tests/`). |
| `app/src-tauri` | Tauri 2 shell for Android (and desktop). Outside the Cargo workspace. |
| `app/src-tauri/plugins/reminders` | Android reminders: alarms, notification buttons, boot receiver (D-62). |
| `tools/climate-build` | Rebuilds `crates/core/data/us-zip-climate.bin`; see [DATA-SOURCES.md](DATA-SOURCES.md). |
| `library/` | The appliance library (CC BY 4.0) at a pinned commit; see "The appliance library" below. |
| `tools/library-sync` | Refreshes `library/`, checks it, and builds library release files. |

## Rust checks

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p housewright-web --target wasm32-unknown-unknown -- -D warnings
cargo test --workspace
```

Do not turn on incremental compilation or set a shared `CARGO_TARGET_DIR`;
`sccache` is the shared cache.

Floor plans (Phase 8): `cargo test -p housewright-plan` (geometry, property
tests, shared vectors), `cargo test -p housewright-core --test
plan_convergence` (concurrent plan edits converge), `cargo test -p
housewright-sync --test plan_sync` (two devices editing one plan through
the real server) and `cargo test -p housewright-api --test plan` (commands,
undo, binder pages). `HW_BINDER_OUT=dir` writes the plan PDFs; look at
them with `pdftoppm -r 70 -png file.pdf out`. If you change a length
format or parse rule, change `crates/plan/src/units.rs`,
`app/src/lib/plan/geometry.ts` and `crates/plan/tests/vectors.json`
together.

Sync's tests (Phase 6) run the real server code in-process
(`crates/sync/tests/common/mod.rs`) and over a real socket
(`crates/sync/tests/http.rs`, `crates/api/tests/sync.rs`). The three-device
property test is `cargo test -p housewright-sync --test convergence`. To try
sync by hand:

```sh
cargo run -p housewright-server -- --listen 127.0.0.1:8787 --data-dir /tmp/hw-server
# then in the app: Settings, Sync and devices, http://127.0.0.1:8787
```

The browser test `app/tests/sync.spec.ts` starts `target/debug/housewright-server`
itself (build it first with `cargo build -p housewright-server`, or set
`HW_SERVER_BIN`). The Android emulator reaches a server on the host at
`http://10.0.2.2:8787`; only debug builds accept that address (D-96).

The OCR, image and QR crates are compiled optimised even in debug and test
builds (`[profile.dev.package]` in the workspace `Cargo.toml` and in
`app/src-tauri/Cargo.toml`); unoptimised, reading one label takes minutes.
`cargo test -p housewright-media --test ocr` reads a synthetic rating plate
with the bundled models.

The house binder's end-to-end tests are in `crates/api/tests/binder.rs`.
They check PDF text with poppler's `pdftotext` when it is installed (and
the laid-out text from Typst always). To look at the PDFs a realistic demo
household produces:

```sh
HW_BINDER_OUT=/tmp/binder cargo test -p housewright-api --test binder
pdftoppm -r 60 -png /tmp/binder/full-main.pdf /tmp/binder/page
```

Typst's crates are compiled optimised in dev and test builds too (as the
OCR crates are); a debug build of the whole workspace with full debug
information is several gigabytes, so on a small disk use
`CARGO_PROFILE_DEV_DEBUG=0 cargo test --workspace`.

## Home Assistant integration

Python is used only here (D-117). Everything stays inside
`integrations/home-assistant/`: a virtual environment in `.venv` (git
ignores it), no global installs. The test harness
(`pytest-homeassistant-custom-component`, pinned in `requirements-dev.txt`
to Home Assistant 2026.10.4) needs Python 3.14; if the system has an older
one, `uv` fetches 3.14 into the folder:

```sh
cd integrations/home-assistant
python3 -m venv .bootstrap && .bootstrap/bin/pip install uv
UV_PYTHON_INSTALL_DIR=$PWD/.uv-python .bootstrap/bin/uv python install 3.14
.bootstrap/bin/uv venv --python 3.14 .venv
.bootstrap/bin/uv pip install --python .venv/bin/python -r requirements-dev.txt
```

The tests start the real `target/debug/housewright-bridge` (with
`housewright-bridge demo`, a sample household) on 127.0.0.1, so build it
first; `test_files.py` also reads the server's and bridge's `--help`:

```sh
cargo build -p housewright-bridge -p housewright-server
.venv/bin/ruff format --check . && .venv/bin/ruff check .
.venv/bin/mypy custom_components/housewright
.venv/bin/python -m pytest -q
```

The shapes the integration relies on are in
`tests/fixtures/bridge-schema.json`, written by
`UPDATE_FIXTURES=1 cargo test -p housewright-bridge --test api`; both sides
check against it (D-126). The browser test `app/tests/ha.spec.ts` pairs a
bridge with a household made in the browser (it starts
`target/debug/housewright-server` and `housewright-bridge`, or
`HW_SERVER_BIN` and `HW_BRIDGE_BIN`).

To try the integration by hand without a household of your own:

```sh
cargo run -p housewright-bridge -- --data-dir /tmp/hwb demo
cargo run -p housewright-bridge -- --data-dir /tmp/hwb serve   # token: /tmp/hwb/api-token
```

## The appliance library

`library/` is a copy of the data in
[holdTheDoorHoid/housewright-library](https://github.com/holdTheDoorHoid/housewright-library)
at one commit (recorded in `library/SNAPSHOT.toml`). Never edit it by
hand: fix the library upstream, then refresh the copy.

```sh
# From a local checkout of the library (it must have no uncommitted data changes):
cargo run -p housewright-library-sync -- copy --from ../housewright-library
# Or from a tagged release on GitHub:
cargo run -p housewright-library-sync -- fetch --tag v2026.12.1
# Check what is in library/ (the tests do this too):
cargo run -p housewright-library-sync -- check
cargo test -p housewright-api --test library
```

The version defaults to the commit's date (`2026.11.28`); pass
`--version 2026.11.28.1` for a second refresh on one day. Both commands
refuse a library that fails the app's checks (for example a family that
extends a template the app does not have): fix that first, upstream or in
`templates/`. Commit `library/` together with the code that needs it.

The library's maintainers publish updates for the app's "Check now" with

```sh
cargo run -p housewright-library-sync -- bundle --out published/
```

which writes `latest.json` and `library-<version>.json.gz` for the
library repository's `published` branch (Q-23). `app/tests/library.spec.ts`
builds the same files in the test and serves them in place of GitHub, so
the tests never reach the network.

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

`npm run wasm` builds both WebAssembly modules (`web/pkg` and
`web-binder/pkg`); a release build of the Typst one takes several minutes,
mostly in `wasm-opt`. The worker imports the binder module dynamically, so
Vite emits it as a separate chunk that is fetched on the first print.

Use `localhost`: OPFS needs a secure context. Only one tab can own the
database at a time. A second tab shows "Already open elsewhere".

Interface checks:

```sh
npm run check          # svelte-check (TypeScript), no warnings allowed
npm test               # vitest: Today flow, reschedule/review, formatting, command list
npm run build:web && npx playwright test   # drives the real web build in Chrome
```

`app/tests/phase9.spec.ts` (contractors, warranties, recalls, the
insurance inventory) never reaches the real CPSC service: it answers every
request to `https://www.saferproducts.gov/` with the recorded response in
`crates/core/tests/fixtures/cpsc/` and checks that only dates were asked
for. It also writes screenshots and the inventory PDF to
`docs/phase-9-evidence/`. To try recall checks against the live service,
turn them on in the app (Home, Recalls); the first check downloads about
13 MB in yearly parts.

`HW_BINDER_OUT=dir cargo test -p housewright-api --test binder` writes every
binder PDF and the insurance inventory (`insurance.pdf`,
`insurance-sensitive.pdf`) to `dir`, to look at.

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
