"""Diagnostics, with anything personal or secret redacted."""

from __future__ import annotations

from typing import Any

from homeassistant.components.diagnostics import async_redact_data
from homeassistant.const import CONF_URL
from homeassistant.core import HomeAssistant

from .const import CONF_DEFAULT_MEMBER, CONF_TOKEN
from .coordinator import HousewrightConfigEntry

TO_REDACT = {
    CONF_TOKEN,
    CONF_URL,
    CONF_DEFAULT_MEMBER,
    "household",
    "server",
    "name",
    "title",
    "item_name",
    "room_name",
    "owner_name",
    "make",
    "model",
    "member",
    "item",
    "entity_id",
}


async def async_get_config_entry_diagnostics(
    hass: HomeAssistant, entry: HousewrightConfigEntry
) -> dict[str, Any]:
    data = entry.runtime_data.data
    return {
        "entry": {
            "data": async_redact_data(dict(entry.data), TO_REDACT),
            "options": async_redact_data(dict(entry.options), TO_REDACT),
        },
        "status": async_redact_data(data.status, TO_REDACT),
        "tasks": [async_redact_data(t, TO_REDACT) for t in data.tasks.values()],
        "items": [async_redact_data(i, TO_REDACT) for i in data.items.values()],
        "members": len(data.members),
    }
