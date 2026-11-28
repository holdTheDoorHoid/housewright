"""Housewright: home maintenance tasks in Home Assistant.

Talks to a `housewright-bridge` (a Housewright device written in Rust)
over its local API. See integrations/home-assistant/README.md.
"""

from __future__ import annotations

from homeassistant.const import CONF_URL, Platform
from homeassistant.core import HomeAssistant
from homeassistant.helpers.aiohttp_client import async_get_clientsession
import homeassistant.helpers.config_validation as cv
from homeassistant.helpers.typing import ConfigType

from .api import BridgeClient
from .const import CONF_TOKEN, DOMAIN
from .coordinator import HousewrightConfigEntry, HousewrightCoordinator
from .links import async_setup_links
from .services import async_setup_services

PLATFORMS = [Platform.BUTTON, Platform.CALENDAR, Platform.SENSOR, Platform.TODO]

CONFIG_SCHEMA = cv.config_entry_only_config_schema(DOMAIN)


async def async_setup(hass: HomeAssistant, config: ConfigType) -> bool:
    async_setup_services(hass)
    return True


async def async_setup_entry(hass: HomeAssistant, entry: HousewrightConfigEntry) -> bool:
    client = BridgeClient(
        async_get_clientsession(hass), entry.data[CONF_URL], entry.data[CONF_TOKEN]
    )
    coordinator = HousewrightCoordinator(hass, entry, client)
    await coordinator.async_config_entry_first_refresh()
    entry.runtime_data = coordinator
    await hass.config_entries.async_forward_entry_setups(entry, PLATFORMS)
    entry.async_on_unload(async_setup_links(hass, entry))
    return True


async def async_unload_entry(hass: HomeAssistant, entry: HousewrightConfigEntry) -> bool:
    return await hass.config_entries.async_unload_platforms(entry, PLATFORMS)
