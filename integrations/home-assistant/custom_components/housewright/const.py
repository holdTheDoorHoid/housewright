"""Constants for the Housewright integration."""

from __future__ import annotations

from datetime import timedelta
from typing import Final

DOMAIN: Final = "housewright"

CONF_TOKEN: Final = "token"
CONF_SCAN_INTERVAL: Final = "scan_interval"
CONF_DEFAULT_MEMBER: Final = "default_member"
CONF_LINKS: Final = "links"

DEFAULT_URL: Final = "http://127.0.0.1:8788"
DEFAULT_SCAN_INTERVAL: Final = 60
MIN_SCAN_INTERVAL: Final = 15
REQUEST_TIMEOUT: Final = 20

SCAN_INTERVAL: Final = timedelta(seconds=DEFAULT_SCAN_INTERVAL)

# The states a task can be in, as the bridge reports them.
STATES: Final = ["ok", "due_soon", "due", "overdue", "snoozed", "paused"]
# States that put a task on the to-do list.
TODO_STATES: Final = {"due_soon", "due", "overdue"}

LINK_METER: Final = "meter"
LINK_EVENT: Final = "event"
