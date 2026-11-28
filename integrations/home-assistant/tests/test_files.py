"""Static checks: manifest, HACS file, translations, blueprints and the add-on."""

from __future__ import annotations

import json
from pathlib import Path
import re
import subprocess

from homeassistant.components.automation.config import AUTOMATION_BLUEPRINT_SCHEMA
from homeassistant.components.blueprint.models import Blueprint
from homeassistant.util.yaml.loader import load_yaml
import pytest
import yaml

from .conftest import BIN, ROOT

HERE = Path(__file__).resolve().parents[1]
COMP = HERE / "custom_components" / "housewright"
ADDON = HERE / "addon" / "housewright"
SERVER_BIN = BIN.parent / "housewright-server"


def cargo_version() -> str:
    text = (ROOT / "Cargo.toml").read_text()
    return re.search(r'^\[workspace\.package\]\nversion = "([^"]+)"', text, re.M).group(1)


def test_manifest_and_hacs() -> None:
    m = json.loads((COMP / "manifest.json").read_text())
    assert m["domain"] == "housewright"
    assert m["config_flow"] is True
    assert m["iot_class"] == "local_polling"
    assert m["requirements"] == [], "no runtime dependencies beyond Home Assistant"
    assert m["version"] == cargo_version()
    h = json.loads((HERE / "hacs.json").read_text())
    assert re.fullmatch(r"\d{4}\.\d+\.\d+", h["homeassistant"])


def test_translations_match_strings() -> None:
    strings = json.loads((COMP / "strings.json").read_text())
    en = json.loads((COMP / "translations" / "en.json").read_text())
    assert strings == en
    services = yaml.safe_load((COMP / "services.yaml").read_text())
    for name, svc in services.items():
        assert name in strings["services"], name
        for field in svc["fields"]:
            assert field in strings["services"][name]["fields"], (name, field)
    # Calm wording (design principles): no "overdue" in any name shown.
    for key, value in strings["entity"]["sensor"]["task_state"]["state"].items():
        assert "overdue" not in value.lower(), key


@pytest.mark.parametrize("path", sorted((HERE / "blueprints").rglob("*.yaml")))
def test_blueprints_are_valid(path: Path) -> None:
    bp = Blueprint(
        load_yaml(path), expected_domain="automation", schema=AUTOMATION_BLUEPRINT_SCHEMA
    )
    assert bp.inputs


def test_addon_config() -> None:
    cfg = yaml.safe_load((ADDON / "config.yaml").read_text())
    for key in ("name", "version", "slug", "description", "arch", "image", "options", "schema"):
        assert key in cfg, key
    assert cfg["version"] == cargo_version()
    assert set(cfg["options"]) <= set(cfg["schema"])
    assert set(cfg["arch"]) <= {"amd64", "aarch64", "armv7", "armhf", "i386"}
    simple = re.compile(
        r"^(bool|str|url|email|port|password|int(\(\d*,\d*\))?|float|list\(.+\))\??$"
    )
    for k, v in cfg["schema"].items():
        assert simple.match(str(v)), (k, v)
    tr = yaml.safe_load((ADDON / "translations" / "en.yaml").read_text())
    assert set(tr["configuration"]) == set(cfg["schema"])
    # The server's registration token stays on (D-105).
    assert cfg["options"]["open_registration"] is False
    repo = yaml.safe_load((HERE / "addon" / "repository.yaml").read_text())
    assert repo["name"]


def test_addon_files_and_flags() -> None:
    docker = (ADDON / "Dockerfile").read_text()
    for src in re.findall(r"^COPY (?!--from)(\S+) ", docker, re.M):
        if src != ".":
            assert (ROOT / src).exists(), src
    run = (ADDON / "run.sh").read_text()
    subprocess.run(["sh", "-n", str(ADDON / "run.sh")], check=True)
    for exe, flags in (
        (SERVER_BIN, re.findall(r"housewright-server.*", run)),
        (BIN, re.findall(r"housewright-bridge .*", run)),
    ):
        if not exe.exists():
            pytest.fail(f"{exe} not built")
        helptext = subprocess.run([str(exe), "--help"], capture_output=True, text=True).stdout
        helptext += subprocess.run(
            [str(exe), "serve", "--help"], capture_output=True, text=True
        ).stdout
        used = set(re.findall(r"--[a-z][a-z-]+", run))
        known = set(re.findall(r"--[a-z][a-z-]+", helptext))
        server_flags = {
            "--listen",
            "--data-dir",
            "--household-quota-mb",
            "--open-registration",
            "--tls-cert",
            "--tls-key",
            "--behind-proxy",
        }
        mine = server_flags if exe == SERVER_BIN else used - server_flags
        assert mine <= known, (exe.name, mine - known, flags)
