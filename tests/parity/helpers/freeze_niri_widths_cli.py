#!/usr/bin/env python3
"""Freeze the observer's CLI grammar in an unsupported, disposable session."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

import freeze_niri_widths as reference

DESTINATION = (
    reference.ROOT / "crates/nacre-helper/tests/fixtures/niri-widths-cli-reference.json"
)


def requests():
    yield []
    yield ["--help"]
    yield ["--check-session"]
    yield ["--check-s"]
    yield ["--"]
    yield ["--unknown"]
    yield ["--check-session=true"]
    yield ["--opening-width"]
    for value in (
        "50",
        "0",
        "101",
        "-1",
        "nan",
        "inf",
        "bad",
        "5_0",
        "5__0",
        "٥٠",
        ".5",
        "1e2",
    ):
        yield ["--opening-width", value]
    yield ["--opening-width=25"]
    yield ["--opening=25"]


def capture(destination=DESTINATION):
    reference.verify_source()
    if destination.exists():
        raise ValueError("Refusing to overwrite frozen width CLI")
    cases = []
    for args in requests():
        with tempfile.TemporaryDirectory(prefix="nacre-widths-cli-") as folder:
            env = dict(
                os.environ,
                HOME=folder,
                XDG_CONFIG_HOME=folder + "/.config",
                XDG_STATE_HOME=folder + "/.local/state",
                XDG_CACHE_HOME=folder + "/.cache",
                XDG_RUNTIME_DIR=folder + "/runtime",
                NACRE_COMPOSITOR="hyprland",
                DBUS_SESSION_BUS_ADDRESS="unix:path=" + folder + "/absent-session",
                DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + folder + "/absent-system",
                PYTHONDONTWRITEBYTECODE="1",
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
                check=False,
            )
            files = {
                str(path.relative_to(folder)): path.read_bytes().hex()
                for path in Path(folder).rglob("*")
                if path.is_file()
            }
            cases.append(
                {
                    "request": {"args": args},
                    "observed": {
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "exit": result.returncode,
                        "files": files,
                    },
                }
            )
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("x") as stream:
        json.dump(
            {"source_sha256": reference.SOURCE_SHA, "cases": cases},
            stream,
            indent=2,
            ensure_ascii=False,
        )
        stream.write("\n")
    print(f"Froze {len(cases)} width CLI cases")


if __name__ == "__main__":
    capture()
