"""A small client for the Housewright bridge's local API.

All Housewright logic lives in the bridge (Rust). This only sends JSON.
"""

from __future__ import annotations

from typing import Any

import aiohttp

from .const import REQUEST_TIMEOUT


class BridgeError(Exception):
    """The bridge answered with an error."""

    def __init__(self, code: str, message: str, status: int = 0) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.status = status


class BridgeAuthError(BridgeError):
    """The token is missing or wrong."""


class BridgeConnectionError(BridgeError):
    """The bridge could not be reached."""


class BridgeClient:
    """Talks to one bridge."""

    def __init__(self, session: aiohttp.ClientSession, url: str, token: str) -> None:
        self._session = session
        self.url = url.rstrip("/")
        self._token = token

    async def _request(self, method: str, path: str, body: Any = None) -> Any:
        headers = {"Authorization": f"Bearer {self._token}"}
        try:
            async with self._session.request(
                method,
                f"{self.url}/api/v1/{path}",
                json=body,
                headers=headers,
                timeout=aiohttp.ClientTimeout(total=REQUEST_TIMEOUT),
            ) as resp:
                try:
                    data = await resp.json(content_type=None)
                except ValueError:
                    data = None
                if resp.status == 401:
                    raise BridgeAuthError("unauthorized", "The bridge's token is wrong.", 401)
                if resp.status >= 400:
                    err = (data or {}).get("error", {}) if isinstance(data, dict) else {}
                    raise BridgeError(
                        err.get("code", "error"),
                        err.get("message", f"The bridge answered {resp.status}."),
                        resp.status,
                    )
                return data
        except (TimeoutError, aiohttp.ClientError, OSError) as err:
            raise BridgeConnectionError(
                "cannot_connect", f"Cannot reach the bridge at {self.url}: {err}"
            ) from err

    async def health(self) -> dict[str, Any]:
        """Unauthenticated: is a bridge there at all."""
        return await self._request("GET", "health")

    async def status(self) -> dict[str, Any]:
        return await self._request("GET", "status")

    async def snapshot(self) -> dict[str, Any]:
        return await self._request("GET", "snapshot")

    async def complete(self, task: str, member: str | None = None) -> dict[str, Any]:
        body: dict[str, Any] = {"task": task}
        if member:
            body["member"] = member
        return await self._request("POST", "complete", body)

    async def snooze(
        self, task: str, until: str | None = None, days: int | None = None
    ) -> dict[str, Any]:
        body: dict[str, Any] = {"task": task}
        if until:
            body["until"] = until
        if days is not None:
            body["days"] = days
        return await self._request("POST", "snooze", body)

    async def reading(
        self,
        item: str,
        meter: str,
        value: float,
        delta: bool = False,
        throttle: bool = False,
    ) -> dict[str, Any]:
        return await self._request(
            "POST",
            "readings",
            {
                "item": item,
                "meter": meter,
                "value": value,
                "delta": delta,
                "throttle": throttle,
            },
        )

    async def event(self, item: str, event: str) -> dict[str, Any]:
        return await self._request("POST", "events", {"item": item, "event": event})

    async def match_devices(self, devices: list[dict[str, Any]]) -> list[dict[str, Any]]:
        return await self._request("POST", "devices/match", {"devices": devices})

    async def import_devices(self, devices: list[dict[str, Any]]) -> dict[str, Any]:
        return await self._request("POST", "devices/import", {"devices": devices})

    async def pair_start(self, server: str, code: str) -> dict[str, Any]:
        return await self._request("POST", "pairing", {"server": server, "code": code})

    async def pair_poll(self) -> dict[str, Any]:
        return await self._request("GET", "pairing")

    async def pair_confirm(self, matches: bool) -> dict[str, Any]:
        return await self._request("POST", "pairing/confirm", {"matches": matches})

    async def sync(self) -> dict[str, Any]:
        return await self._request("POST", "sync")
