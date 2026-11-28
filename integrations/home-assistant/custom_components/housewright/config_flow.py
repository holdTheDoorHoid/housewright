"""Setting up: the bridge's address and token, and pairing it if needed.

Options: how often to poll, who "Mark done" is for, importing devices,
and linking sensors to meters and events (usage triggers).
"""

from __future__ import annotations

import asyncio
from collections.abc import Mapping
from typing import Any

from homeassistant.config_entries import (
    ConfigEntry,
    ConfigFlow,
    ConfigFlowResult,
    OptionsFlow,
    OptionsFlowWithReload,
)
from homeassistant.const import CONF_URL
from homeassistant.core import HomeAssistant, callback
from homeassistant.helpers import area_registry as ar, device_registry as dr
from homeassistant.helpers.aiohttp_client import async_get_clientsession
from homeassistant.helpers.selector import (
    BooleanSelector,
    EntitySelector,
    EntitySelectorConfig,
    NumberSelector,
    NumberSelectorConfig,
    NumberSelectorMode,
    SelectOptionDict,
    SelectSelector,
    SelectSelectorConfig,
    TextSelector,
    TextSelectorConfig,
    TextSelectorType,
)
import probatio as vol

from .api import BridgeAuthError, BridgeClient, BridgeConnectionError, BridgeError
from .const import (
    CONF_DEFAULT_MEMBER,
    CONF_LINKS,
    CONF_SCAN_INTERVAL,
    CONF_TOKEN,
    DEFAULT_SCAN_INTERVAL,
    DEFAULT_URL,
    DOMAIN,
    LINK_EVENT,
    LINK_METER,
    MIN_SCAN_INTERVAL,
)

# How long the setup waits for the other side while pairing, in seconds.
PAIR_WAIT = 20
POLL_EVERY = 1.0


def _client(hass: HomeAssistant, url: str, token: str) -> BridgeClient:
    return BridgeClient(async_get_clientsession(hass), url, token)


async def _wait_for(client: BridgeClient, states: set[str]) -> dict[str, Any]:
    """Poll the pairing until it reaches one of `states` (or gives up)."""
    s: dict[str, Any] = {}
    for _ in range(int(PAIR_WAIT / POLL_EVERY)):
        s = await client.pair_poll()
        if s.get("state") in states | {"failed"}:
            return s
        await asyncio.sleep(POLL_EVERY)
    return s


class HousewrightConfigFlow(ConfigFlow, domain=DOMAIN):
    """Address and token; then, for a new bridge, the pairing code."""

    VERSION = 1

    def __init__(self) -> None:
        self._url = DEFAULT_URL
        self._token = ""
        self._safety = ""

    @property
    def _bridge(self) -> BridgeClient:
        return _client(self.hass, self._url, self._token)

    async def async_step_user(self, user_input: dict[str, Any] | None = None) -> ConfigFlowResult:
        errors: dict[str, str] = {}
        if user_input is not None:
            self._url = user_input[CONF_URL].strip().rstrip("/")
            self._token = user_input[CONF_TOKEN].strip()
            try:
                status = await self._bridge.status()
            except BridgeAuthError:
                errors[CONF_TOKEN] = "invalid_auth"
            except BridgeConnectionError:
                errors[CONF_URL] = "cannot_connect"
            except BridgeError:
                errors["base"] = "not_a_bridge"
            else:
                if status.get("service") != "housewright-bridge":
                    errors["base"] = "not_a_bridge"
                elif not status.get("paired"):
                    return await self.async_step_pair()
                else:
                    return await self._finish(status)
        return self.async_show_form(
            step_id="user",
            data_schema=vol.Schema(
                {
                    vol.Required(CONF_URL, default=self._url): TextSelector(
                        TextSelectorConfig(type=TextSelectorType.URL)
                    ),
                    vol.Required(CONF_TOKEN): TextSelector(
                        TextSelectorConfig(type=TextSelectorType.PASSWORD)
                    ),
                }
            ),
            errors=errors,
        )

    async def _finish(self, status: dict[str, Any]) -> ConfigFlowResult:
        member = status.get("member") or {}
        await self.async_set_unique_id(member.get("id"))
        self._abort_if_unique_id_configured(updates={CONF_URL: self._url, CONF_TOKEN: self._token})
        return self.async_create_entry(
            title=status.get("household") or "Housewright",
            data={CONF_URL: self._url, CONF_TOKEN: self._token},
        )

    async def async_step_pair(self, user_input: dict[str, Any] | None = None) -> ConfigFlowResult:
        """The code from the app: Settings, Sync and devices, Add a device or
        person, Home Assistant."""
        errors: dict[str, str] = {}
        placeholders = {"error": ""}
        if user_input is not None:
            try:
                await self._bridge.pair_start(
                    user_input.get("server", "").strip(), user_input["code"].strip()
                )
                s = await _wait_for(self._bridge, {"compare"})
            except BridgeConnectionError:
                errors["base"] = "cannot_connect"
            except BridgeError as err:
                errors["base"] = "pairing_failed"
                placeholders["error"] = err.message
            else:
                if s.get("state") == "compare":
                    self._safety = s.get("safety_number", "")
                    return await self.async_step_compare()
                errors["base"] = "pairing_failed"
                placeholders["error"] = s.get("reason", "The app did not answer in time.")
        return self.async_show_form(
            step_id="pair",
            data_schema=vol.Schema(
                {
                    vol.Optional("server", default=""): TextSelector(
                        TextSelectorConfig(type=TextSelectorType.URL)
                    ),
                    vol.Required("code"): str,
                }
            ),
            errors=errors,
            description_placeholders=placeholders,
        )

    async def async_step_compare(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        """Show the safety number; the person compares it with the app."""
        if user_input is not None:
            matches = bool(user_input.get("matches"))
            try:
                await self._bridge.pair_confirm(matches)
            except BridgeError as err:
                return self.async_abort(
                    reason="pairing_stopped", description_placeholders={"error": err.message}
                )
            if not matches:
                return self.async_abort(
                    reason="pairing_stopped", description_placeholders={"error": ""}
                )
            return await self.async_step_finish()
        return self.async_show_form(
            step_id="compare",
            data_schema=vol.Schema({vol.Required("matches", default=False): BooleanSelector()}),
            description_placeholders={"safety_number": self._safety},
        )

    async def async_step_finish(self, user_input: dict[str, Any] | None = None) -> ConfigFlowResult:
        """Wait for the owner to tap "They match" in the app."""
        errors: dict[str, str] = {}
        if user_input is not None:
            s = await _wait_for(self._bridge, {"done"})
            if s.get("state") == "done":
                return await self._finish(await self._bridge.status())
            if s.get("state") == "failed":
                return self.async_abort(
                    reason="pairing_stopped",
                    description_placeholders={"error": s.get("reason", "")},
                )
            errors["base"] = "not_finished"
        return self.async_show_form(step_id="finish", data_schema=vol.Schema({}), errors=errors)

    async def async_step_reauth(self, entry_data: Mapping[str, Any]) -> ConfigFlowResult:
        self._url = entry_data[CONF_URL]
        return await self.async_step_reauth_confirm()

    async def async_step_reauth_confirm(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        errors: dict[str, str] = {}
        if user_input is not None:
            self._token = user_input[CONF_TOKEN].strip()
            try:
                await self._bridge.status()
            except BridgeAuthError:
                errors[CONF_TOKEN] = "invalid_auth"
            except BridgeError:
                errors["base"] = "cannot_connect"
            else:
                return self.async_update_reload_and_abort(
                    self._get_reauth_entry(), data_updates={CONF_TOKEN: self._token}
                )
        return self.async_show_form(
            step_id="reauth_confirm",
            data_schema=vol.Schema(
                {
                    vol.Required(CONF_TOKEN): TextSelector(
                        TextSelectorConfig(type=TextSelectorType.PASSWORD)
                    )
                }
            ),
            errors=errors,
        )

    @staticmethod
    @callback
    def async_get_options_flow(config_entry: ConfigEntry) -> OptionsFlow:
        return HousewrightOptionsFlow()


class HousewrightOptionsFlow(OptionsFlowWithReload):
    """Settings, device import and usage triggers."""

    def __init__(self) -> None:
        self._candidates: dict[str, dict[str, Any]] = {}
        self._imported = ""

    @property
    def _coordinator(self) -> Any:
        return self.config_entry.runtime_data

    def _save(self, **changes: Any) -> ConfigFlowResult:
        return self.async_create_entry(data={**self.config_entry.options, **changes})

    async def async_step_init(self, user_input: dict[str, Any] | None = None) -> ConfigFlowResult:
        menu = ["settings", "import_devices", "link_meter", "link_event"]
        if self.config_entry.options.get(CONF_LINKS):
            menu.append("remove_links")
        return self.async_show_menu(step_id="init", menu_options=menu)

    async def async_step_settings(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        if user_input is not None:
            return self._save(
                **{
                    CONF_SCAN_INTERVAL: int(user_input[CONF_SCAN_INTERVAL]),
                    CONF_DEFAULT_MEMBER: user_input.get(CONF_DEFAULT_MEMBER, ""),
                }
            )
        people = [
            SelectOptionDict(value=m["id"], label=m["name"])
            for m in self._coordinator.data.members
            if not m.get("home_assistant")
        ]
        opts = self.config_entry.options
        return self.async_show_form(
            step_id="settings",
            data_schema=vol.Schema(
                {
                    vol.Required(
                        CONF_SCAN_INTERVAL,
                        default=opts.get(CONF_SCAN_INTERVAL, DEFAULT_SCAN_INTERVAL),
                    ): NumberSelector(
                        NumberSelectorConfig(
                            min=MIN_SCAN_INTERVAL,
                            max=3600,
                            step=1,
                            mode=NumberSelectorMode.BOX,
                            unit_of_measurement="s",
                        )
                    ),
                    vol.Optional(
                        CONF_DEFAULT_MEMBER,
                        description={"suggested_value": opts.get(CONF_DEFAULT_MEMBER) or None},
                    ): SelectSelector(SelectSelectorConfig(options=people)),
                }
            ),
        )

    # -- device import ----------------------------------------------------

    def _ha_devices(self) -> list[dict[str, Any]]:
        devices = dr.async_get(self.hass)
        areas = ar.async_get(self.hass)
        out = []
        for d in devices.devices:
            if d.entry_type is dr.DeviceEntryType.SERVICE or d.disabled:
                continue
            if any(i[0] == DOMAIN for i in d.identifiers):
                continue
            area = areas.async_get_area(d.area_id) if d.area_id else None
            out.append(
                {
                    "id": d.id,
                    "name": d.name_by_user or d.name or "",
                    "manufacturer": d.manufacturer,
                    "model": d.model,
                    "area": area.name if area else None,
                }
            )
        return [d for d in out if d["name"]]

    async def async_step_import_devices(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        """Devices that look like something Housewright knows (a washer, a
        smoke alarm) and are not recorded yet."""
        client = self._coordinator.client
        if user_input is not None:
            chosen = [
                self._candidates[i] for i in user_input.get("devices", []) if i in self._candidates
            ]
            try:
                out = await client.import_devices(chosen)
            except BridgeError as err:
                return self.async_abort(
                    reason="bridge_error", description_placeholders={"error": err.message}
                )
            await self._coordinator.async_request_refresh()
            self._imported = ", ".join(a["name"] for a in out["added"]) or "-"
            return await self.async_step_imported()
        devices = self._ha_devices()
        try:
            matched = await client.match_devices(devices) if devices else []
        except BridgeError as err:
            return self.async_abort(
                reason="bridge_error", description_placeholders={"error": err.message}
            )
        by_id = {d["id"]: d for d in devices}
        self._candidates = {}
        options = []
        for m in matched:
            if not m.get("template") or m.get("existing_item"):
                continue
            d = dict(by_id[m["id"]], template=m["template"])
            self._candidates[m["id"]] = d
            where = f", {d['area']}" if d.get("area") else ""
            options.append(
                SelectOptionDict(value=m["id"], label=f"{d['name']} ({m['template_name']}{where})")
            )
        if not options:
            return self.async_abort(reason="no_devices")
        return self.async_show_form(
            step_id="import_devices",
            data_schema=vol.Schema(
                {
                    vol.Optional("devices", default=[]): SelectSelector(
                        SelectSelectorConfig(options=options, multiple=True)
                    )
                }
            ),
        )

    async def async_step_imported(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        if user_input is not None:
            return self._save()
        return self.async_show_form(
            step_id="imported",
            data_schema=vol.Schema({}),
            description_placeholders={"added": self._imported},
        )

    # -- usage triggers ---------------------------------------------------

    def _items(self, need_meters: bool) -> list[SelectOptionDict]:
        out = []
        for i in self._coordinator.data.items.values():
            if need_meters:
                for m in i["meters"]:
                    out.append(
                        SelectOptionDict(
                            value=f"{i['id']}|{m['key']}", label=f"{i['name']}: {m['name']}"
                        )
                    )
            else:
                out.append(SelectOptionDict(value=i["id"], label=i["name"]))
        return out

    def _add_link(self, link: dict[str, Any]) -> ConfigFlowResult:
        links = [
            x
            for x in self.config_entry.options.get(CONF_LINKS, [])
            if not (x["entity_id"] == link["entity_id"] and x["kind"] == link["kind"])
        ]
        return self._save(**{CONF_LINKS: [*links, link]})

    async def async_step_link_meter(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        """A sensor whose number is a meter (HVAC runtime hours, washer cycles)."""
        if user_input is not None:
            item, meter = user_input["target"].split("|", 1)
            return self._add_link(
                {
                    "kind": LINK_METER,
                    "entity_id": user_input["entity_id"],
                    "item": item,
                    "meter": meter,
                }
            )
        targets = self._items(need_meters=True)
        if not targets:
            return self.async_abort(reason="no_meters")
        return self.async_show_form(
            step_id="link_meter",
            data_schema=vol.Schema(
                {
                    vol.Required("entity_id"): EntitySelector(
                        EntitySelectorConfig(domain=["sensor", "counter", "input_number", "number"])
                    ),
                    vol.Required("target"): SelectSelector(SelectSelectorConfig(options=targets)),
                }
            ),
        )

    async def async_step_link_event(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        """An entity whose state means something happened (a leak, a low battery)."""
        if user_input is not None:
            return self._add_link(
                {
                    "kind": LINK_EVENT,
                    "entity_id": user_input["entity_id"],
                    "item": user_input["item"],
                    "event": user_input["event"],
                    "on": user_input.get("on") or "on",
                }
            )
        items = self._items(need_meters=False)
        if not items:
            return self.async_abort(reason="no_items")
        return self.async_show_form(
            step_id="link_event",
            data_schema=vol.Schema(
                {
                    vol.Required("entity_id"): EntitySelector(
                        EntitySelectorConfig(domain=["binary_sensor", "sensor", "input_boolean"])
                    ),
                    vol.Required("item"): SelectSelector(SelectSelectorConfig(options=items)),
                    vol.Required("event", default="leak"): SelectSelector(
                        SelectSelectorConfig(options=["leak", "battery_low"], custom_value=True)
                    ),
                    vol.Optional("on", default="on"): str,
                }
            ),
        )

    async def async_step_remove_links(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        links = list(self.config_entry.options.get(CONF_LINKS, []))
        if user_input is not None:
            drop = set(user_input.get("links", []))
            keep = [x for n, x in enumerate(links) if str(n) not in drop]
            return self._save(**{CONF_LINKS: keep})
        names = {i["id"]: i["name"] for i in self._coordinator.data.items.values()}
        options = [
            SelectOptionDict(
                value=str(n),
                label=f"{x['entity_id']} -> {names.get(x['item'], 'a thing')}: {x.get('meter') or x.get('event')}",
            )
            for n, x in enumerate(links)
        ]
        return self.async_show_form(
            step_id="remove_links",
            data_schema=vol.Schema(
                {
                    vol.Optional("links", default=[]): SelectSelector(
                        SelectSelectorConfig(options=options, multiple=True)
                    )
                }
            ),
        )
