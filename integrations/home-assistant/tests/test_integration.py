"""The integration against a real housewright-bridge (see conftest.py)."""

from __future__ import annotations

from datetime import timedelta
import json

from homeassistant import config_entries
from homeassistant.components.todo import TodoItemStatus
from homeassistant.const import CONF_URL
from homeassistant.core import HomeAssistant
from homeassistant.data_entry_flow import FlowResultType
from homeassistant.helpers import area_registry as ar, device_registry as dr, entity_registry as er
from homeassistant.util import dt as dt_util
import pytest
from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.housewright.const import CONF_LINKS, CONF_TOKEN, DOMAIN
from custom_components.housewright.diagnostics import async_get_config_entry_diagnostics

from .conftest import SCHEMA, TOKEN, Bridge


async def setup(
    hass: HomeAssistant, bridge: Bridge, options: dict | None = None
) -> MockConfigEntry:
    entry = MockConfigEntry(
        domain=DOMAIN,
        title="Demo household",
        data={CONF_URL: bridge.url, CONF_TOKEN: bridge.token},
        options=options or {},
        unique_id="demo",
    )
    entry.add_to_hass(hass)
    assert await hass.config_entries.async_setup(entry.entry_id)
    await hass.async_block_till_done()
    return entry


def entity_for(hass: HomeAssistant, platform: str, unique_suffix: str) -> str:
    reg = er.async_get(hass)
    for e in reg.entities.values():
        if e.platform == DOMAIN and e.domain == platform and e.unique_id.endswith(unique_suffix):
            return e.entity_id
    raise AssertionError(f"no {platform} entity ending {unique_suffix}")


def task_id(bridge: Bridge, title: str) -> str:
    return next(t["id"] for t in bridge.get("tasks") if t["title"] == title)


async def refresh(hass: HomeAssistant, entry: MockConfigEntry) -> None:
    await entry.runtime_data.async_refresh()
    await hass.async_block_till_done()


def all_states_text(hass: HomeAssistant) -> str:
    return json.dumps([s.as_dict() for s in hass.states.async_all()], default=str)


# -- config flow --------------------------------------------------------------


async def test_config_flow_success(hass: HomeAssistant, bridge: Bridge) -> None:
    result = await hass.config_entries.flow.async_init(
        DOMAIN, context={"source": config_entries.SOURCE_USER}
    )
    assert result["type"] is FlowResultType.FORM
    result = await hass.config_entries.flow.async_configure(
        result["flow_id"], {CONF_URL: bridge.url + "/", CONF_TOKEN: f" {TOKEN} "}
    )
    assert result["type"] is FlowResultType.CREATE_ENTRY
    assert result["title"] == "Demo household"
    assert result["data"] == {CONF_URL: bridge.url, CONF_TOKEN: TOKEN}
    # The same bridge again is refused.
    again = await hass.config_entries.flow.async_init(
        DOMAIN, context={"source": config_entries.SOURCE_USER}
    )
    again = await hass.config_entries.flow.async_configure(
        again["flow_id"], {CONF_URL: bridge.url, CONF_TOKEN: TOKEN}
    )
    assert again["type"] is FlowResultType.ABORT
    assert again["reason"] == "already_configured"


async def test_config_flow_bad_token(hass: HomeAssistant, bridge: Bridge) -> None:
    result = await hass.config_entries.flow.async_init(
        DOMAIN, context={"source": config_entries.SOURCE_USER}
    )
    result = await hass.config_entries.flow.async_configure(
        result["flow_id"], {CONF_URL: bridge.url, CONF_TOKEN: "wrong-token"}
    )
    assert result["type"] is FlowResultType.FORM
    assert result["errors"] == {CONF_TOKEN: "invalid_auth"}


async def test_config_flow_unreachable(hass: HomeAssistant, socket_enabled) -> None:
    from .conftest import _free_port

    result = await hass.config_entries.flow.async_init(
        DOMAIN, context={"source": config_entries.SOURCE_USER}
    )
    result = await hass.config_entries.flow.async_configure(
        result["flow_id"], {CONF_URL: f"http://127.0.0.1:{_free_port()}", CONF_TOKEN: TOKEN}
    )
    assert result["type"] is FlowResultType.FORM
    assert result["errors"] == {CONF_URL: "cannot_connect"}


async def test_config_flow_unpaired_bridge_asks_for_a_code(
    hass: HomeAssistant, unpaired_bridge: Bridge
) -> None:
    result = await hass.config_entries.flow.async_init(
        DOMAIN, context={"source": config_entries.SOURCE_USER}
    )
    result = await hass.config_entries.flow.async_configure(
        result["flow_id"], {CONF_URL: unpaired_bridge.url, CONF_TOKEN: TOKEN}
    )
    assert result["type"] is FlowResultType.FORM
    assert result["step_id"] == "pair"
    # A code for a server that is not there: a plain error, not a crash.
    result = await hass.config_entries.flow.async_configure(
        result["flow_id"],
        {"server": "http://127.0.0.1:9", "code": "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF"},
    )
    assert result["type"] is FlowResultType.FORM
    assert result["errors"] == {"base": "pairing_failed"}
    assert result["description_placeholders"]["error"]


async def test_reauth_with_a_new_token(hass: HomeAssistant, bridge: Bridge) -> None:
    entry = await setup(hass, bridge)
    result = await entry.start_reauth_flow(hass)
    assert result["step_id"] == "reauth_confirm"
    bad = await hass.config_entries.flow.async_configure(result["flow_id"], {CONF_TOKEN: "nope"})
    assert bad["errors"] == {CONF_TOKEN: "invalid_auth"}
    ok = await hass.config_entries.flow.async_configure(result["flow_id"], {CONF_TOKEN: TOKEN})
    assert ok["type"] is FlowResultType.ABORT
    assert ok["reason"] == "reauth_successful"
    await hass.async_block_till_done()
    assert await hass.config_entries.async_unload(entry.entry_id)


# -- entities -----------------------------------------------------------------


async def test_sensors_states_and_attributes(hass: HomeAssistant, bridge: Bridge) -> None:
    await setup(hass, bridge)
    tid = task_id(bridge, "Test the smoke alarm")
    state = hass.states.get(entity_for(hass, "sensor", f"{tid}_state"))
    assert state.state == "due"
    assert state.attributes["device_class"] == "enum"
    assert "overdue" in state.attributes["options"]
    assert state.attributes["item"] == "Smoke alarm"
    assert state.attributes["room"] == "Hallway"
    assert state.attributes["owner"] == "Ana"
    assert state.attributes["days_until"] == 0
    assert state.attributes["due"] == state.attributes["earliest"] or state.attributes["due"]
    summary = hass.states.get(entity_for(hass, "sensor", "_summary"))
    tasks = bridge.get("tasks")
    ready = sum(1 for t in tasks if t["state"] in ("due_soon", "due", "overdue"))
    assert int(summary.state) == ready
    assert (
        summary.attributes["due"] + summary.attributes["due_soon"] + summary.attributes["overdue"]
        == ready
    )
    assert summary.attributes["switched_on"] == len(tasks)
    # No alarming wording in names.
    assert "overdue" not in (summary.attributes.get("friendly_name") or "").lower()
    # Per-thing devices, in their areas.
    devices = dr.async_get(hass)
    names = {d.name for d in devices.devices if any(i[0] == DOMAIN for i in d.identifiers)}
    assert {"Demo household", "Smoke alarm", "Washing machine", "Tank water heater"} <= names
    # Nothing sensitive reached Home Assistant.
    assert "SECRET" not in all_states_text(hass)


async def test_todo_check_off_completes_the_task(hass: HomeAssistant, bridge: Bridge) -> None:
    await setup(hass, bridge)
    todo = entity_for(hass, "todo", "_todo")
    items = await hass.services.async_call(
        "todo", "get_items", {}, target={"entity_id": todo}, blocking=True, return_response=True
    )
    listed = {i["summary"]: i for i in items[todo]["items"]}
    assert "Test the smoke alarm" in listed
    assert "Run a washer cleaning cycle" not in listed, "not due yet"
    assert listed["Test the smoke alarm"]["description"] == "Smoke alarm, Hallway (Ana)"
    await hass.services.async_call(
        "todo",
        "update_item",
        {"item": listed["Test the smoke alarm"]["uid"], "status": TodoItemStatus.COMPLETED},
        target={"entity_id": todo},
        blocking=True,
    )
    await hass.async_block_till_done()
    t = next(t for t in bridge.get("tasks") if t["title"] == "Test the smoke alarm")
    assert t["state"] == "ok"
    assert t["last_done"]
    items = await hass.services.async_call(
        "todo", "get_items", {}, target={"entity_id": todo}, blocking=True, return_response=True
    )
    assert "Test the smoke alarm" not in {i["summary"] for i in items[todo]["items"]}


async def test_calendar_has_due_windows(hass: HomeAssistant, bridge: Bridge) -> None:
    await setup(hass, bridge)
    cal = entity_for(hass, "calendar", "_calendar")
    now = dt_util.now()
    out = await hass.services.async_call(
        "calendar",
        "get_events",
        {"start_date_time": now - timedelta(days=1), "end_date_time": now + timedelta(days=90)},
        target={"entity_id": cal},
        blocking=True,
        return_response=True,
    )
    summaries = [e["summary"] for e in out[cal]["events"]]
    assert "Test the smoke alarm" in summaries
    assert "Run a washer cleaning cycle" in summaries
    ev = next(e for e in out[cal]["events"] if e["summary"] == "Run a washer cleaning cycle")
    assert "Washing machine, Laundry" in ev["description"]
    assert hass.states.get(cal).state in ("on", "off")


async def test_button_marks_done(hass: HomeAssistant, bridge: Bridge) -> None:
    await setup(hass, bridge, options={"default_member": "Ben"})
    tid = task_id(bridge, "Inspect the anode rod")
    await hass.services.async_call(
        "button", "press", {"entity_id": entity_for(hass, "button", f"{tid}_done")}, blocking=True
    )
    await hass.async_block_till_done()
    assert hass.states.get(entity_for(hass, "sensor", f"{tid}_state")).state == "ok"


# -- services -----------------------------------------------------------------


async def test_services(hass: HomeAssistant, bridge: Bridge) -> None:
    entry = await setup(hass, bridge)
    out = await hass.services.async_call(
        DOMAIN,
        "complete",
        {"task": "Replace the smoke alarm battery", "member": "Ben"},
        blocking=True,
        return_response=True,
    )
    assert out["state"] == "ok"
    out = await hass.services.async_call(
        DOMAIN,
        "snooze",
        {"task": "Inspect the washer supply hoses", "days": 7},
        blocking=True,
        return_response=True,
    )
    assert out["state"] == "snoozed"
    await refresh(hass, entry)
    tid = task_id(bridge, "Inspect the washer supply hoses")
    assert hass.states.get(entity_for(hass, "sensor", f"{tid}_state")).state == "snoozed"
    out = await hass.services.async_call(
        DOMAIN,
        "add_meter_reading",
        {"item": "Washing machine", "meter": "loads", "value": 3},
        blocking=True,
        return_response=True,
    )
    assert out["stored"] is True
    # An event switches on the suggested leak task, which appears in Home Assistant.
    out = await hass.services.async_call(
        DOMAIN,
        "report_event",
        {"item": "Washing machine", "event": "leak"},
        blocking=True,
        return_response=True,
    )
    assert out["recorded"] is True
    assert out["tasks"][0]["state"] == "due"
    await refresh(hass, entry)
    leak = out["tasks"][0]["id"]
    assert hass.states.get(entity_for(hass, "sensor", f"{leak}_state")).state == "due"
    # Errors are plain.
    from homeassistant.exceptions import HomeAssistantError

    with pytest.raises(HomeAssistantError, match="No task called"):
        await hass.services.async_call(
            DOMAIN, "complete", {"task": "Nothing like this"}, blocking=True
        )


# -- options ------------------------------------------------------------------


async def test_options_device_import(hass: HomeAssistant, bridge: Bridge) -> None:
    entry = await setup(hass, bridge)
    other = MockConfigEntry(domain="test")
    other.add_to_hass(hass)
    area = ar.async_get(hass).async_create("Kitchen")
    devices = dr.async_get(hass)
    dw = devices.async_get_or_create(
        config_entry_id=other.entry_id,
        identifiers={("test", "dw")},
        name="Dishwasher",
        manufacturer="Bosch",
        model="SHPM88Z75N",
    )
    devices.async_update_device(dw.id, area_id=area.id)
    devices.async_get_or_create(
        config_entry_id=other.entry_id, identifiers={("test", "bulb")}, name="Lamp", model="LCT015"
    )
    result = await hass.config_entries.options.async_init(entry.entry_id)
    assert result["type"] is FlowResultType.MENU
    result = await hass.config_entries.options.async_configure(
        result["flow_id"], {"next_step_id": "import_devices"}
    )
    assert result["step_id"] == "import_devices"
    schema = result["data_schema"].schema
    options = next(iter(schema.values())).config["options"]
    assert [o["label"] for o in options] == ["Dishwasher (Dishwasher, Kitchen)"], (
        "the lamp is not offered"
    )
    result = await hass.config_entries.options.async_configure(
        result["flow_id"], {"devices": [dw.id]}
    )
    assert result["step_id"] == "imported"
    assert result["description_placeholders"]["added"] == "Dishwasher"
    result = await hass.config_entries.options.async_configure(result["flow_id"], {})
    assert result["type"] is FlowResultType.CREATE_ENTRY
    item = next(i for i in bridge.get("items") if i["name"] == "Dishwasher")
    assert (item["make"], item["model"], item["room_name"], item["template"]) == (
        "Bosch",
        "SHPM88Z75N",
        "Kitchen",
        "dishwasher",
    )


async def test_options_event_link_reports_a_leak(hass: HomeAssistant, bridge: Bridge) -> None:
    entry = await setup(hass, bridge)
    heater = next(i for i in bridge.get("items") if i["name"] == "Tank water heater")
    hass.states.async_set("binary_sensor.heater_leak", "off")
    result = await hass.config_entries.options.async_init(entry.entry_id)
    result = await hass.config_entries.options.async_configure(
        result["flow_id"], {"next_step_id": "link_event"}
    )
    result = await hass.config_entries.options.async_configure(
        result["flow_id"],
        {
            "entity_id": "binary_sensor.heater_leak",
            "item": heater["id"],
            "event": "leak",
            "on": "on",
        },
    )
    assert result["type"] is FlowResultType.CREATE_ENTRY
    await hass.async_block_till_done()
    assert entry.options[CONF_LINKS][0]["event"] == "leak"
    hass.states.async_set("binary_sensor.heater_leak", "on")
    await hass.async_block_till_done()
    t = next(
        t
        for t in bridge.get("tasks")
        if t["title"] == "Check the tank water heater after a leak alert"
    )
    assert t["state"] == "due"


async def test_options_meter_link_sends_readings(hass: HomeAssistant, bridge: Bridge) -> None:
    washer = next(i for i in bridge.get("items") if i["name"] == "Washing machine")
    hass.states.async_set("counter.washer_cycles", "5")
    entry = await setup(hass, bridge)
    result = await hass.config_entries.options.async_init(entry.entry_id)
    result = await hass.config_entries.options.async_configure(
        result["flow_id"], {"next_step_id": "link_meter"}
    )
    result = await hass.config_entries.options.async_configure(
        result["flow_id"], {"entity_id": "counter.washer_cycles", "target": f"{washer['id']}|loads"}
    )
    assert result["type"] is FlowResultType.CREATE_ENTRY
    await hass.async_block_till_done()
    hass.states.async_set("counter.washer_cycles", "6")
    await hass.async_block_till_done()
    meter = next(i for i in bridge.get("items") if i["name"] == "Washing machine")["meters"][0]
    # The first push (5, after the reload's poll) is stored; 6 comes too
    # soon after it and waits for a later poll (throttled).
    assert meter["last_value"] in (5.0, 6.0)


async def test_options_settings(hass: HomeAssistant, bridge: Bridge) -> None:
    entry = await setup(hass, bridge)
    ben = next(m for m in bridge.get("members") if m["name"] == "Ben")
    result = await hass.config_entries.options.async_init(entry.entry_id)
    result = await hass.config_entries.options.async_configure(
        result["flow_id"], {"next_step_id": "settings"}
    )
    result = await hass.config_entries.options.async_configure(
        result["flow_id"], {"scan_interval": 120, "default_member": ben["id"]}
    )
    assert result["type"] is FlowResultType.CREATE_ENTRY
    await hass.async_block_till_done()
    assert entry.runtime_data.update_interval == timedelta(seconds=120)


# -- diagnostics and the contract ---------------------------------------------


async def test_diagnostics_are_redacted(hass: HomeAssistant, bridge: Bridge) -> None:
    entry = await setup(hass, bridge, options={"default_member": "Ben"})
    diag = await async_get_config_entry_diagnostics(hass, entry)
    text = json.dumps(diag, default=str)
    for secret in (
        TOKEN,
        bridge.url,
        "Demo household",
        "Smoke alarm",
        "Ana",
        "Ben",
        "Hallway",
        "SECRET",
    ):
        assert secret not in text, secret
    assert diag["tasks"] and diag["tasks"][0]["state"]


async def test_bridge_matches_the_shared_schema(bridge: Bridge) -> None:
    """The real bridge's answers have exactly the keys in the fixture the
    Rust tests write (crates/bridge/tests/api.rs)."""
    snap = bridge.get("snapshot")
    assert sorted(snap) == SCHEMA["snapshot"]
    assert sorted(snap["status"]) == SCHEMA["status"]
    assert sorted(snap["status"]["sync"]) == SCHEMA["status.sync"]
    for t in snap["tasks"]:
        assert sorted(t) == SCHEMA["task"]
        assert t["state"] in SCHEMA["states"]
    for i in snap["items"]:
        assert sorted(i) == SCHEMA["item"]
        for m in i["meters"]:
            assert sorted(m) == SCHEMA["meter"]
    for m in snap["members"]:
        assert sorted(m) == SCHEMA["member"]
    from custom_components.housewright.const import STATES

    assert SCHEMA["states"] == STATES
