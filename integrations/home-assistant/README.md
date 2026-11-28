# Housewright for Home Assistant

Your home maintenance plan in Home Assistant (D-26, D-117):

- a **Maintenance** to-do list of tasks ready to do (checking one off marks
  it done) and a **Maintenance** calendar of due windows;
- a sensor per task (`ok`, `due_soon`, `due`, `overdue`, `snoozed`,
  `paused`, shown as "OK", "Due soon", "Due", "Past due", ...) with the due
  date, days until, thing, room and owner, a "Mark done" button per task,
  and a "Tasks ready to do" summary sensor with counts;
- services `housewright.complete`, `housewright.snooze`,
  `housewright.add_meter_reading` and `housewright.report_event`, which
  take a task or thing by id or exact name;
- **usage triggers**: link a sensor to a meter (HVAC runtime hours, washer
  cycles) or an entity to an event (a leak sensor, a low-battery sensor),
  in the integration's options or with the blueprints in `blueprints/`;
- **device import**: add Home Assistant devices that look like a washer, a
  smoke alarm, a water heater... as things, with their tasks as suggestions;
- an **add-on** (`addon/`) that runs the sync server and the bridge.

Tested with Home Assistant 2026.10.4 (the current test harness); the
minimum declared in `hacs.json` is 2026.10.0. It uses no Python packages
beyond Home Assistant itself.

## How it fits together

Sync is end-to-end encrypted, so Home Assistant cannot read your household
from the server. It goes through the **bridge** (`housewright-bridge`,
Rust, `crates/bridge`): a Housewright device without a screen that joins
your household like a phone does, keeps its own copy and keys, syncs, and
offers a small JSON API on `127.0.0.1:8788` protected by a token. This
integration is only glue that calls that API. All Housewright logic stays
in Rust. Read "What Home Assistant can see" in
[docs/SECURITY.md](../../docs/SECURITY.md) before connecting it: the bridge
holds the household's keys on the Home Assistant machine.

```
phone / browser  ──►  sync server  ◄──  housewright-bridge  ◄── this integration ── Home Assistant
 (Housewright app)    (ciphertext)       (a member device,       (HTTP + token,
                                          keys, SQLite)           127.0.0.1)
```

## Setting it up

You need sync turned on in the app (Settings, Sync and devices). The add-on
can be your sync server.

1. **Run the bridge.**
   - Home Assistant OS: install the Housewright add-on (see
     [docs/SELF-HOSTING.md](../../docs/SELF-HOSTING.md#home-assistant-add-on)).
     Its log shows the bridge's address and token.
   - Otherwise, on the machine running Home Assistant:
     ```sh
     cargo build --release --locked -p housewright-bridge
     ./target/release/housewright-bridge --data-dir ~/.local/share/housewright-bridge serve
     ./target/release/housewright-bridge --data-dir ~/.local/share/housewright-bridge token
     ```
2. **Install the integration**: copy `custom_components/housewright` into
   Home Assistant's `config/custom_components/` (HACS: see Q-12 in
   DECISIONS.md), restart Home Assistant, then Settings, Devices and
   services, Add integration, Housewright. Enter the bridge's address and
   token.
3. **Pair**: Home Assistant asks for a code. In the app: Sync and devices,
   Add a device or person, **Home Assistant**. Type the code, compare the
   six-digit number on both, tick "The numbers match", then tap "They
   match" in the app and press Submit.

The bridge can also pair from a terminal:
`housewright-bridge join --server https://your.server --code ABCD-...`.

## Events and meters

| What | How |
|---|---|
| A leak sensor under the water heater | Options, "Report an event when a sensor changes": the `binary_sensor`, the thing, event `leak`, state `on`. "Check the tank water heater after a leak alert" becomes due (it is switched on if it was only a suggestion). |
| A smoke alarm's low-battery entity | Same, event `battery_low`. |
| HVAC runtime hours, washer cycles | Options, "Send a sensor's number to a meter". Usage tasks ("every 40 loads") come due when the meter has gone up enough. At most one reading every 30 minutes is kept. |
| From an automation | `housewright.report_event` / `housewright.add_meter_reading`, or the blueprints in `blueprints/automation/housewright/`. |

Events are lowercase names (`leak`, `battery_low`, or your own); a task
follows an event when its schedule is `{ kind = "triggered", event = "leak" }`.

## Assist

The Maintenance to-do list works with Home Assistant's built-in list
sentences ("what is on my maintenance list", "check off ... on the
maintenance list"). No custom intents yet (D-125); not checked with a
voice pipeline.

## Development

See [docs/DEVELOPING.md](../../docs/DEVELOPING.md#home-assistant-integration).
In short, from this folder:

```sh
python3 -m venv .bootstrap && .bootstrap/bin/pip install uv
UV_PYTHON_INSTALL_DIR=$PWD/.uv-python .bootstrap/bin/uv python install 3.14
.bootstrap/bin/uv venv --python 3.14 .venv
.bootstrap/bin/uv pip install --python .venv/bin/python -r requirements-dev.txt
cargo build -p housewright-bridge -p housewright-server   # the tests start the real bridge
.venv/bin/ruff format --check . && .venv/bin/ruff check . && .venv/bin/mypy custom_components/housewright
.venv/bin/python -m pytest -q
```
