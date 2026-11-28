"""A calendar of each task's due window."""

from __future__ import annotations

from datetime import date, datetime, timedelta

from homeassistant.components.calendar import CalendarEntity, CalendarEvent
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddConfigEntryEntitiesCallback
from homeassistant.helpers.update_coordinator import CoordinatorEntity
from homeassistant.util import dt as dt_util

from .coordinator import HousewrightConfigEntry, HousewrightCoordinator
from .entity import household_device
from .todo import describe


async def async_setup_entry(
    hass: HomeAssistant,
    entry: HousewrightConfigEntry,
    async_add_entities: AddConfigEntryEntitiesCallback,
) -> None:
    async_add_entities([MaintenanceCalendar(entry.runtime_data)])


def event_of(task: dict) -> CalendarEvent | None:
    """All-day, from when doing it counts to the day before it is overdue."""
    if not task.get("due") or task["state"] == "paused":
        return None
    start = date.fromisoformat(task.get("earliest") or task["due"])
    last = date.fromisoformat(task.get("overdue_after") or task["due"])
    end = max(last, start) + timedelta(days=1)
    text = describe(task)
    due = task["due"]
    return CalendarEvent(
        start=start,
        end=end,
        summary=task["title"],
        description=f"{text}. Due {due}." if text else f"Due {due}.",
        uid=task["id"],
    )


def _as_date(d: datetime | date) -> date:
    return d.date() if isinstance(d, datetime) else d


class MaintenanceCalendar(CoordinatorEntity[HousewrightCoordinator], CalendarEntity):
    """Due windows of switched-on tasks."""

    _attr_has_entity_name = True
    _attr_translation_key = "maintenance"

    def __init__(self, coordinator: HousewrightCoordinator) -> None:
        super().__init__(coordinator)
        self._attr_unique_id = f"{coordinator.config_entry.entry_id}_calendar"
        self._attr_device_info = household_device(coordinator)

    def _events(self) -> list[CalendarEvent]:
        out = [e for t in self.coordinator.data.tasks.values() if (e := event_of(t))]
        out.sort(key=lambda e: (e.start, e.summary))
        return out

    @property
    def event(self) -> CalendarEvent | None:
        today = dt_util.now().date()
        return next((e for e in self._events() if _as_date(e.end) > today), None)

    async def async_get_events(
        self, hass: HomeAssistant, start_date: datetime, end_date: datetime
    ) -> list[CalendarEvent]:
        lo, hi = _as_date(start_date), _as_date(end_date)
        return [e for e in self._events() if _as_date(e.start) <= hi and _as_date(e.end) > lo]
