"""Polls the bridge for the household's tasks, things and people."""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import timedelta
import logging
from typing import Any

from homeassistant.config_entries import ConfigEntry
from homeassistant.core import HomeAssistant
from homeassistant.exceptions import ConfigEntryAuthFailed
from homeassistant.helpers.update_coordinator import DataUpdateCoordinator, UpdateFailed

from .api import BridgeAuthError, BridgeClient, BridgeError
from .const import CONF_SCAN_INTERVAL, DEFAULT_SCAN_INTERVAL, DOMAIN, MIN_SCAN_INTERVAL

_LOGGER = logging.getLogger(__name__)


@dataclass
class Snapshot:
    """One poll's worth of data."""

    status: dict[str, Any]
    tasks: dict[str, dict[str, Any]] = field(default_factory=dict)
    items: dict[str, dict[str, Any]] = field(default_factory=dict)
    members: list[dict[str, Any]] = field(default_factory=list)

    @property
    def household(self) -> str:
        return self.status.get("household") or "Housewright"


type HousewrightConfigEntry = ConfigEntry[HousewrightCoordinator]


class HousewrightCoordinator(DataUpdateCoordinator[Snapshot]):
    """Fetches `/api/v1/snapshot` every minute (by default)."""

    config_entry: HousewrightConfigEntry

    def __init__(
        self, hass: HomeAssistant, entry: HousewrightConfigEntry, client: BridgeClient
    ) -> None:
        seconds = max(
            MIN_SCAN_INTERVAL,
            int(entry.options.get(CONF_SCAN_INTERVAL, DEFAULT_SCAN_INTERVAL)),
        )
        super().__init__(
            hass,
            _LOGGER,
            config_entry=entry,
            name=DOMAIN,
            update_interval=timedelta(seconds=seconds),
        )
        self.client = client

    async def _async_update_data(self) -> Snapshot:
        try:
            data = await self.client.snapshot()
        except BridgeAuthError as err:
            raise ConfigEntryAuthFailed(err.message) from err
        except BridgeError as err:
            raise UpdateFailed(err.message) from err
        return Snapshot(
            status=data["status"],
            tasks={t["id"]: t for t in data["tasks"]},
            items={i["id"]: i for i in data["items"]},
            members=data["members"],
        )
