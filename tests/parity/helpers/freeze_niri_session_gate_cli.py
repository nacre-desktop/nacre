#!/usr/bin/env python3
"""Freeze gate CLI stdout/exits in a disposable unknown session."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import freeze_niri_session_gate as reference

DESTINATION = (
    reference.ROOT
    / "crates/nacre-helper/tests/fixtures/niri-session-gate-cli-reference.json"
)


def capture(destination=DESTINATION):
    reference.verify_source()
    if destination.exists():
        raise ValueError("Refusing to overwrite frozen gate CLI")
    cases = []
    for args in (
        [],
        ["shell"],
        ["observer"],
        ["idle"],
        ["unknown"],
        ["--help"],
        ["-h"],
        ["shell", "--help"],
        ["unknown", "--help"],
        ["shell", "idle"],
        ["shell", "idle", "--help"],
        ["--"],
        ["--", "shell"],
        ["--bad"],
    ):
        with tempfile.TemporaryDirectory(prefix="nacre-gate-cli-") as folder:
            env = dict(
                os.environ,
                HOME=folder,
                XDG_CONFIG_HOME=folder + "/.config",
                XDG_STATE_HOME=folder + "/.local/state",
                XDG_CACHE_HOME=folder + "/.cache",
                DBUS_SESSION_BUS_ADDRESS="unix:path=" + folder + "/absent-session",
                DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + folder + "/absent-system",
            )
            for key in (
                "DISPLAY",
                "WAYLAND_DISPLAY",
                "NIRI_SOCKET",
                "HYPRLAND_INSTANCE_SIGNATURE",
                "XDG_CURRENT_DESKTOP",
            ):
                env.pop(key, None)
            result = subprocess.run(
                [sys.executable, str(reference.SOURCE), *args],
                env=env,
                capture_output=True,
                text=True,
            )
            cases.append(
                {
                    "request": {"args": args},
                    "observed": {
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "exit": result.returncode,
                        "files": {
                            str(path.relative_to(folder)): path.read_bytes().hex()
                            for path in Path(folder).rglob("*")
                            if path.is_file()
                        },
                    },
                }
            )
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("x") as stream:
        json.dump(
            {"source_sha256": reference.SOURCE_SHA, "cases": cases}, stream, indent=2
        )
        stream.write("\n")
    print(f"Froze {len(cases)} gate CLI cases")


if __name__ == "__main__":
    capture()
