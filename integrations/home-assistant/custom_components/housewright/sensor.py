"""A sensor per task (its state) and a summary sensor (counts)."""

from __future__ import annotations

from typing import Any

from homeassistant.components.sensor import SensorDeviceClass, SensorEntity
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddConfigEntryEntitiesCallback
from homeassistant.helpers.update_coordinator import CoordinatorEntity

from .const import STATES, TODO_STATES
from .coordinator import HousewrightConfigEntry, HousewrightCoordinator
from .entity import TaskEntity, add_new_tasks, household_device


async def async_setup_entry(
    hass: HomeAssistant,
    entry: HousewrightConfigEntry,
    async_add_entities: AddConfigEntryEntitiesCallback,
) -> None:
    coordinator = entry.runtime_data
    async_add_entities([SummarySensor(coordinator)])
    entry.async_on_unload(
        add_new_tasks(
            coordinator,
            lambda task_id: [TaskStateSensor(coordinator, task_id)],
            async_add_entities,
        )
    )


class TaskStateSensor(TaskEntity, SensorEntity):
    """ok, due_soon, due, overdue, snoozed or paused."""

    _attr_device_class = SensorDeviceClass.ENUM
    _attr_options = STATES
    _attr_translation_key = "task_state"

    def __init__(self, coordinator: HousewrightCoordinator, task_id: str) -> None:
        super().__init__(coordinator, task_id, "state")
        self._attr_name = coordinator.data.tasks[task_id]["title"]

    @property
    def native_value(self) -> str | None:
        return self.task["state"] if self.task else None

    @property
    def extra_state_attributes(self) -> dict[str, Any]:
        t = self.task or {}
        return {
            "task_id": self.task_id,
            "due": t.get("due"),
            "earliest": t.get("earliest"),
            "overdue_after": t.get("overdue_after"),
            "days_until": t.get("days_until"),
            "last_done": t.get("last_done"),
            "item": t.get("item_name"),
            "room": t.get("room_name"),
            "owner": t.get("owner_name"),
            "priority": t.get("priority_label"),
            "schedule": t.get("schedule_text"),
            "minutes": t.get("minutes"),
        }


class SummarySensor(CoordinatorEntity[HousewrightCoordinator], SensorEntity):
    """How many tasks are ready to do, with counts by state."""

    _attr_has_entity_name = True
    _attr_translation_key = "summary"
    _attr_native_unit_of_measurement = "tasks"

    def __init__(self, coordinator: HousewrightCoordinator) -> None:
        super().__init__(coordinator)
        self._attr_unique_id = f"{coordinator.config_entry.entry_id}_summary"
        self._attr_device_info = household_device(coordinator)

    def _counts(self) -> dict[str, int]:
        counts = dict.fromkeys(STATES, 0)
        for t in self.coordinator.data.tasks.values():
            counts[t["state"]] = counts.get(t["state"], 0) + 1
        return counts

    @property
    def native_value(self) -> int:
        counts = self._counts()
        return sum(counts[s] for s in TODO_STATES)

    @property
    def extra_state_attributes(self) -> dict[str, Any]:
        counts = self._counts()
        nxt = min(
            (
                t
                for t in self.coordinator.data.tasks.values()
                if t.get("due") and t["state"] not in ("paused",)
            ),
            key=lambda t: t["due"],
            default=None,
        )
        return {
            **counts,
            "switched_on": len(self.coordinator.data.tasks),
            "next_task": nxt["title"] if nxt else None,
            "next_due": nxt["due"] if nxt else None,
        }
