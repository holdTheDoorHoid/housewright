"""Shared entity helpers: devices for the household and for each thing."""

from __future__ import annotations

from collections.abc import Callable
from typing import Any

from homeassistant.helpers.device_registry import DeviceEntryType, DeviceInfo
from homeassistant.helpers.update_coordinator import CoordinatorEntity

from .const import DOMAIN
from .coordinator import HousewrightCoordinator


def household_device(coordinator: HousewrightCoordinator) -> DeviceInfo:
    """The household as a service device."""
    entry = coordinator.config_entry
    return DeviceInfo(
        identifiers={(DOMAIN, entry.entry_id)},
        name=coordinator.data.household if coordinator.data else "Housewright",
        manufacturer="Housewright",
        model="Household",
        entry_type=DeviceEntryType.SERVICE,
        sw_version=(coordinator.data.status.get("version") if coordinator.data else None),
    )


def task_device(coordinator: HousewrightCoordinator, task: dict[str, Any]) -> DeviceInfo:
    """A task's thing (the washing machine), or the household."""
    entry = coordinator.config_entry
    item_id = task.get("item_id")
    if not item_id:
        return household_device(coordinator)
    item = coordinator.data.items.get(item_id, {}) if coordinator.data else {}
    return DeviceInfo(
        identifiers={(DOMAIN, f"{entry.entry_id}_{item_id}")},
        name=task.get("item_name") or item.get("name") or "Thing",
        manufacturer=item.get("make") or "Housewright",
        model=item.get("model"),
        suggested_area=task.get("room_name"),
        entry_type=DeviceEntryType.SERVICE,
    )


class TaskEntity(CoordinatorEntity[HousewrightCoordinator]):
    """An entity about one task."""

    _attr_has_entity_name = True

    def __init__(self, coordinator: HousewrightCoordinator, task_id: str, kind: str) -> None:
        super().__init__(coordinator)
        self.task_id = task_id
        task = coordinator.data.tasks[task_id]
        self._attr_unique_id = f"{coordinator.config_entry.entry_id}_{task_id}_{kind}"
        self._attr_device_info = task_device(coordinator, task)

    @property
    def task(self) -> dict[str, Any] | None:
        if self.coordinator.data is None:
            return None
        return self.coordinator.data.tasks.get(self.task_id)

    @property
    def available(self) -> bool:
        return super().available and self.task is not None


def add_new_tasks(
    coordinator: HousewrightCoordinator,
    make: Callable[[str], list[Any]],
    add: Callable[[list[Any]], None],
) -> Callable[[], None]:
    """Add entities for tasks as they appear (switched on in the app later)."""
    known: set[str] = set()

    def check() -> None:
        if coordinator.data is None:
            return
        new = [t for t in coordinator.data.tasks if t not in known]
        if not new:
            return
        known.update(new)
        entities: list[Any] = []
        for task_id in new:
            entities.extend(make(task_id))
        add(entities)

    check()
    return coordinator.async_add_listener(check)
