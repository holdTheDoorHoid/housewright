"""Services: complete, snooze, add_meter_reading, report_event.

Tasks and things can be given by id or by name; the bridge resolves them.
"""

from __future__ import annotations

from typing import Any

from homeassistant.config_entries import ConfigEntryState
from homeassistant.core import (
    HomeAssistant,
    ServiceCall,
    ServiceResponse,
    SupportsResponse,
    callback,
)
from homeassistant.exceptions import HomeAssistantError, ServiceValidationError
import homeassistant.helpers.config_validation as cv
import probatio as vol

from .api import BridgeError
from .const import CONF_DEFAULT_MEMBER, DOMAIN
from .coordinator import HousewrightConfigEntry

ATTR_ENTRY = "config_entry_id"

BASE: dict[Any, Any] = {vol.Optional(ATTR_ENTRY): cv.string}

COMPLETE_SCHEMA = vol.Schema(
    {**BASE, vol.Required("task"): cv.string, vol.Optional("member"): cv.string}
)
SNOOZE_SCHEMA = vol.Schema(
    {
        **BASE,
        vol.Required("task"): cv.string,
        vol.Exclusive("until", "when"): cv.date,
        vol.Exclusive("days", "when"): vol.All(vol.Coerce(int), vol.Range(min=1, max=366)),
    }
)
READING_SCHEMA = vol.Schema(
    {
        **BASE,
        vol.Required("item"): cv.string,
        vol.Required("meter"): cv.string,
        vol.Required("value"): vol.Coerce(float),
        vol.Optional("delta", default=False): cv.boolean,
    }
)
EVENT_SCHEMA = vol.Schema(
    {**BASE, vol.Required("item"): cv.string, vol.Required("event"): cv.string}
)


def _entry(hass: HomeAssistant, call: ServiceCall) -> HousewrightConfigEntry:
    entries = [
        e for e in hass.config_entries.async_entries(DOMAIN) if e.state is ConfigEntryState.LOADED
    ]
    wanted = call.data.get(ATTR_ENTRY)
    if wanted:
        entries = [e for e in entries if e.entry_id == wanted]
    if len(entries) != 1:
        raise ServiceValidationError(
            translation_domain=DOMAIN,
            translation_key="which_household" if entries else "no_household",
        )
    return entries[0]


@callback
def async_setup_services(hass: HomeAssistant) -> None:
    async def run(call: ServiceCall, fn: Any) -> ServiceResponse:
        entry = _entry(hass, call)
        try:
            out = await fn(entry)
        except BridgeError as err:
            raise HomeAssistantError(err.message) from err
        await entry.runtime_data.async_request_refresh()
        return out if call.return_response else None

    async def complete(call: ServiceCall) -> ServiceResponse:
        return await run(
            call,
            lambda e: e.runtime_data.client.complete(
                call.data["task"],
                call.data.get("member") or e.options.get(CONF_DEFAULT_MEMBER) or None,
            ),
        )

    async def snooze(call: ServiceCall) -> ServiceResponse:
        until = call.data.get("until")
        return await run(
            call,
            lambda e: e.runtime_data.client.snooze(
                call.data["task"],
                until=until.isoformat() if until else None,
                days=call.data.get("days"),
            ),
        )

    async def reading(call: ServiceCall) -> ServiceResponse:
        return await run(
            call,
            lambda e: e.runtime_data.client.reading(
                call.data["item"],
                call.data["meter"],
                call.data["value"],
                delta=call.data["delta"],
            ),
        )

    async def event(call: ServiceCall) -> ServiceResponse:
        return await run(
            call,
            lambda e: e.runtime_data.client.event(call.data["item"], call.data["event"]),
        )

    for name, handler, schema in (
        ("complete", complete, COMPLETE_SCHEMA),
        ("snooze", snooze, SNOOZE_SCHEMA),
        ("add_meter_reading", reading, READING_SCHEMA),
        ("report_event", event, EVENT_SCHEMA),
    ):
        hass.services.async_register(
            DOMAIN, name, handler, schema=schema, supports_response=SupportsResponse.OPTIONAL
        )
