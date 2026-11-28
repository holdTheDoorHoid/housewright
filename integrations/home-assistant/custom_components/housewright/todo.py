"""A to-do list of tasks ready to do. Checking one off marks it done."""

from __future__ import annotations

from datetime import date

from homeassistant.components.todo import (
    TodoItem,
    TodoItemStatus,
    TodoListEntity,
    TodoListEntityFeature,
)
from homeassistant.core import HomeAssistant
from homeassistant.exceptions import HomeAssistantError
from homeassistant.helpers.entity_platform import AddConfigEntryEntitiesCallback
from homeassistant.helpers.update_coordinator import CoordinatorEntity

from .api import BridgeError
from .const import CONF_DEFAULT_MEMBER, TODO_STATES
from .coordinator import HousewrightConfigEntry, HousewrightCoordinator
from .entity import household_device


async def async_setup_entry(
    hass: HomeAssistant,
    entry: HousewrightConfigEntry,
    async_add_entities: AddConfigEntryEntitiesCallback,
) -> None:
    async_add_entities([MaintenanceList(entry.runtime_data)])


def describe(task: dict) -> str:
    parts = [p for p in (task.get("item_name"), task.get("room_name")) if p]
    text = ", ".join(parts)
    if task.get("owner_name"):
        text = f"{text} ({task['owner_name']})" if text else task["owner_name"]
    return text


class MaintenanceList(CoordinatorEntity[HousewrightCoordinator], TodoListEntity):
    """Due soon, due and overdue tasks, most urgent first."""

    _attr_has_entity_name = True
    _attr_translation_key = "maintenance"
    _attr_supported_features = TodoListEntityFeature.UPDATE_TODO_ITEM

    def __init__(self, coordinator: HousewrightCoordinator) -> None:
        super().__init__(coordinator)
        self._attr_unique_id = f"{coordinator.config_entry.entry_id}_todo"
        self._attr_device_info = household_device(coordinator)

    @property
    def todo_items(self) -> list[TodoItem]:
        items = []
        for t in self.coordinator.data.tasks.values():
            if t["state"] not in TODO_STATES:
                continue
            items.append(
                TodoItem(
                    summary=t["title"],
                    uid=t["id"],
                    status=TodoItemStatus.NEEDS_ACTION,
                    due=date.fromisoformat(t["due"]) if t.get("due") else None,
                    description=describe(t) or None,
                )
            )
        return items

    async def async_update_todo_item(self, item: TodoItem) -> None:
        if item.status != TodoItemStatus.COMPLETED:
            # Renaming or un-checking is done in the app, not here.
            return
        member = self.coordinator.config_entry.options.get(CONF_DEFAULT_MEMBER) or None
        try:
            await self.coordinator.client.complete(item.uid or "", member)
        except BridgeError as err:
            raise HomeAssistantError(err.message) from err
        await self.coordinator.async_request_refresh()
