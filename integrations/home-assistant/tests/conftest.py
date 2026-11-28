"""Fixtures: a real housewright-bridge on 127.0.0.1 with a demo household.

The tests talk to the real Rust bridge, not a fake. Build it first:
`cargo build -p housewright-bridge` (or set HW_BRIDGE_BIN).
"""

from __future__ import annotations

from collections.abc import Generator
import json
import os
from pathlib import Path
import socket
import subprocess
import time
import urllib.request

import pytest

ROOT = Path(__file__).resolve().parents[3]
BIN = Path(os.environ.get("HW_BRIDGE_BIN", ROOT / "target" / "debug" / "housewright-bridge"))
TOKEN = "pytest-token-0123456789abcdefghijklmnopqrstuvwxyz"
SCHEMA = json.loads((Path(__file__).parent / "fixtures" / "bridge-schema.json").read_text())


@pytest.fixture(autouse=True)
def auto_enable_custom_integrations(enable_custom_integrations):
    """Load custom_components/ in every test."""
    return


def _free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


class Bridge:
    """A running bridge process."""

    def __init__(self, data: Path, demo: bool) -> None:
        if not BIN.exists():
            pytest.fail(f"{BIN} not found: run `cargo build -p housewright-bridge` first")
        self.data = data
        if demo:
            subprocess.run(
                [str(BIN), "--data-dir", str(data), "demo"], check=True, capture_output=True
            )
        self.port = _free_port()
        self.url = f"http://127.0.0.1:{self.port}"
        self.token = TOKEN
        self.proc = subprocess.Popen(
            [str(BIN), "--data-dir", str(data), "serve", "--listen", f"127.0.0.1:{self.port}"],
            env={**os.environ, "HW_BRIDGE_TOKEN": TOKEN},
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        for _ in range(100):
            try:
                with urllib.request.urlopen(f"{self.url}/api/v1/health", timeout=1):
                    return
            except OSError:
                time.sleep(0.05)
        self.stop()
        pytest.fail("the bridge did not start")

    def get(self, path: str) -> dict:
        req = urllib.request.Request(
            f"{self.url}/api/v1/{path}", headers={"Authorization": f"Bearer {TOKEN}"}
        )
        with urllib.request.urlopen(req, timeout=10) as r:
            return json.loads(r.read())

    def stop(self) -> None:
        self.proc.terminate()
        try:
            self.proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.proc.kill()


@pytest.fixture
def bridge(tmp_path: Path, socket_enabled) -> Generator[Bridge]:
    """A bridge with the demo household (a washer, a smoke alarm, a water
    heater, people Ana and Ben, and sensitive things that must not show)."""
    b = Bridge(tmp_path / "bridge", demo=True)
    yield b
    b.stop()


@pytest.fixture
def unpaired_bridge(tmp_path: Path, socket_enabled) -> Generator[Bridge]:
    b = Bridge(tmp_path / "empty", demo=False)
    yield b
    b.stop()
