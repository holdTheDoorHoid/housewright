"""A "Mark done" button per task."""

from __future__ import annotations

from homeassistant.components.button import ButtonEntity
from homeassistant.core import HomeAssistant
from homeassistant.exceptions import HomeAssistantError
from homeassistant.helpers.entity_platform import AddConfigEntryEntitiesCallback

from .api import BridgeError
from .const import CONF_DEFAULT_MEMBER
from .coordinator import HousewrightConfigEntry, HousewrightCoordinator
from .entity import TaskEntity, add_new_tasks


async def async_setup_entry(
    hass: HomeAssistant,
    entry: HousewrightConfigEntry,
    async_add_entities: AddConfigEntryEntitiesCallback,
) -> None:
    coordinator = entry.runtime_data
    entry.async_on_unload(
        add_new_tasks(
            coordinator,
            lambda task_id: [MarkDoneButton(coordinator, task_id)],
            async_add_entities,
        )
    )


class MarkDoneButton(TaskEntity, ButtonEntity):
    """Marks the task done, for the default person (or Home Assistant)."""

    _attr_translation_key = "mark_done"

    def __init__(self, coordinator: HousewrightCoordinator, task_id: str) -> None:
        super().__init__(coordinator, task_id, "done")
        title = coordinator.data.tasks[task_id]["title"]
        self._attr_name = f"{title}: mark done"

    async def async_press(self) -> None:
        member = self.coordinator.config_entry.options.get(CONF_DEFAULT_MEMBER) or None
        try:
            await self.coordinator.client.complete(self.task_id, member)
        except BridgeError as err:
            raise HomeAssistantError(err.message) from err
        await self.coordinator.async_request_refresh()
