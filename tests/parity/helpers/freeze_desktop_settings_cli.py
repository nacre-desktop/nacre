#!/usr/bin/env python3
"""Freeze original settings CLI parsing without any desktop actions."""

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "nacre/shell-tools/desktop-settings.py"
DEST = ROOT / "crates/nacre-helper/tests/fixtures/desktop-settings-cli-reference.json"

if __name__ == "__main__":
    if DEST.exists():
        raise ValueError("Refusing to overwrite frozen settings CLI outputs")
    cases = []
    for args in (
        [],
        ["--help"],
        ["bad"],
        ["--bad"],
        ["--help=invalid"],
        ["state", "--unknown"],
        ["--h"],
    ):
        with tempfile.TemporaryDirectory(prefix="session-cli-reference-") as home:
            env = dict(
                os.environ,
                HOME=home,
                XDG_CONFIG_HOME=home + "/.config",
                XDG_STATE_HOME=home + "/.local/state",
                XDG_CACHE_HOME=home + "/.cache",
                XDG_DATA_HOME=home + "/.local/share",
                NACRE_COMPOSITOR="niri",
                NIRI_SOCKET=home + "/runtime/niri.sock",
                NIRI_CONFIG=home + "/.config/niri/config.kdl",
                NACRE_HELPER_BINARY=str(ROOT / "target/debug/nacre-helper"),
                COLUMNS="80",
                DBUS_SESSION_BUS_ADDRESS="unix:path=" + home + "/absent-session",
                DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + home + "/absent-system",
            )
            result = subprocess.run(
                [sys.executable, str(SOURCE), *args],
                env=env,
                capture_output=True,
                text=True,
                timeout=5,
            )
            cases.append(
                dict(
                    args=args,
                    stdout=result.stdout,
                    stderr=result.stderr,
                    exit=result.returncode,
                )
            )
    DEST.write_text(
        json.dumps(
            dict(
                source_sha256=hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
                cases=cases,
            ),
            indent=2,
        )
        + "\n"
    )
    print("Frozen", len(cases), "settings CLI cases")
