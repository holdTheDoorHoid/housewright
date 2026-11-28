"""Usage triggers (D-26 item 2): Home Assistant entities linked, in the
options, to a Housewright meter or event.

- A meter link sends the entity's number (runtime hours, washer cycles) as
  a reading whenever it changes, and again after each poll. The bridge stores at most one reading per
  meter every 30 minutes and skips unchanged values.
- An event link reports an event (`leak`, `battery_low`) when the entity
  turns to the given state (default `on`, as a leak or battery
  `binary_sensor` does).
"""

from __future__ import annotations

from collections.abc import Callable
import logging
from typing import Any

from homeassistant.const import STATE_UNAVAILABLE, STATE_UNKNOWN
from homeassistant.core import Event, EventStateChangedData, HomeAssistant, callback
from homeassistant.helpers.event import async_track_state_change_event

from .api import BridgeError
from .const import CONF_LINKS, LINK_EVENT, LINK_METER
from .coordinator import HousewrightConfigEntry

_LOGGER = logging.getLogger(__name__)


def links_of(entry: HousewrightConfigEntry) -> list[dict[str, Any]]:
    return list(entry.options.get(CONF_LINKS, []))


@callback
def async_setup_links(hass: HomeAssistant, entry: HousewrightConfigEntry) -> Callable[[], None]:
    links = links_of(entry)
    if not links:
        return lambda: None
    by_entity: dict[str, list[dict[str, Any]]] = {}
    for link in links:
        by_entity.setdefault(link["entity_id"], []).append(link)

    async def push(link: dict[str, Any], old: str | None, new: str) -> None:
        client = entry.runtime_data.client
        try:
            if link["kind"] == LINK_METER:
                try:
                    value = float(new)
                except ValueError:
                    return
                await client.reading(link["item"], link["meter"], value, throttle=True)
            elif link["kind"] == LINK_EVENT:
                on = link.get("on") or "on"
                if new == on and old != on:
                    await client.event(link["item"], link["event"])
                    await entry.runtime_data.async_request_refresh()
        except BridgeError as err:
            _LOGGER.warning("Housewright: could not send %s: %s", link["entity_id"], err.message)

    @callback
    def changed(event: Event[EventStateChangedData]) -> None:
        new = event.data["new_state"]
        if new is None or new.state in (STATE_UNAVAILABLE, STATE_UNKNOWN):
            return
        old = event.data["old_state"]
        for link in by_entity.get(event.data["entity_id"], []):
            hass.async_create_task(push(link, old.state if old else None, new.state))

    @callback
    def resend() -> None:
        # A reading skipped as "too soon" is sent again with the next poll,
        # so the last value before a sensor goes quiet is not lost.
        for link in links:
            if link["kind"] != LINK_METER:
                continue
            state = hass.states.get(link["entity_id"])
            if state is not None and state.state not in (STATE_UNAVAILABLE, STATE_UNKNOWN):
                hass.async_create_task(push(link, None, state.state))

    stop_states = async_track_state_change_event(hass, list(by_entity), changed)
    stop_polls = entry.runtime_data.async_add_listener(resend)

    def stop() -> None:
        stop_states()
        stop_polls()

    return stop
