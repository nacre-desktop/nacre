#!/usr/bin/env python3
"""Freeze lock preparation with public system/clock inputs and actual rendering."""

import gzip
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
from unittest.mock import patch

from PIL import Image
from colors_reference_files import Files
from lock_font_fixture import setup, fingerprints

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "nacre/shell-tools/lock-prepare.py"
SOURCE_SHA = "418531dd81439f1c71ed4e94f35e9d38e72b3555dea74375e2074eacdd4cde0c"
DEST = (
    ROOT / "crates/nacre-helper/tests/fixtures/lock-prepare-isolated-reference.json.gz"
)


def main():
    if DEST.exists():
        raise ValueError("Refusing to replace lock preparation references")
    if hashlib.sha256(SOURCE.read_bytes()).hexdigest() != SOURCE_SHA:
        raise ValueError("Current lock preparation source changed")
    sys.path[:0] = [str(SOURCE.parent), str(SOURCE.parent / "tests")]
    from native_lock_dashboard import invoke

    cases = []
    for name in ("empty", "notifications", "artwork-disabled"):
        with tempfile.TemporaryDirectory(
            prefix="nacre-lock-prepare-original-"
        ) as folder:
            home = Path(folder) / "home"
            home.mkdir()
            env = dict(
                HOME=str(home),
                XDG_CONFIG_HOME=str(home / ".config"),
                XDG_DATA_HOME=str(home / ".local/share"),
                XDG_STATE_HOME=str(home / ".local/state"),
                XDG_CACHE_HOME=str(home / ".cache"),
                XDG_RUNTIME_DIR=str(home / "runtime"),
                NACRE_LANGUAGE="en",
                NACRE_HELPER_TOOLS=str(SOURCE.parent),
                NACRE_COMPOSITOR="hyprland",
                DBUS_SESSION_BUS_ADDRESS="unix:path=" + str(home / "absent-bus"),
                DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + str(home / "absent-system-bus"),
                NIRI_SOCKET=str(home / "absent-niri"),
                HYPRLAND_INSTANCE_SIGNATURE="absent-fixture",
            )
            font_env, font_inputs = setup(Path(folder), home)
            env.update(font_env)
            provider_before = fingerprints(Path(folder))
            logo = home / ".local/share/nacre/branding/nacre-lock.png"
            logo.parent.mkdir(parents=True)
            Image.new("RGBA", (32, 24), (23, 43, 69, 255)).save(logo)
            state = home / ".local/state/nacre"
            state.mkdir(parents=True)
            colors = json.loads((SOURCE.parent / "reference-style.json").read_text())[
                "colours"
            ]
            (state / "scheme.json").write_text(json.dumps(dict(colours=colors)))
            cache = home / ".cache/nacre"
            cache.mkdir(parents=True)
            monitors = [dict(name="fixture", width=640, height=360)]
            (cache / "lock-outputs.json").write_text(json.dumps(monitors))
            power = home / "fixture-sys/power_supply/BAT0"
            power.mkdir(parents=True)
            (power / "capacity").write_text("63\n")
            data = dict(preferences={})
            if name != "empty":
                data.update(
                    count=1,
                    notifications=[
                        dict(
                            app="Fixture Mail",
                            summary="Private title",
                            body="Private body",
                        )
                    ],
                    weather=dict(
                        location="Oslo", temperature="14 °C", description="Cloudy"
                    ),
                )
            if name == "artwork-disabled":
                data["preferences"]["lockMedia"] = False
            for index in range(14):
                path = cache / f"lock-art-old-{index:02}.png"
                path.write_bytes(logo.read_bytes())
                os.utime(path, (index, index))
            files = Files(home, [])
            inputs = files.snapshot()
            with patch.dict(os.environ, env):
                spec = importlib.util.spec_from_file_location(
                    "original_lock_prepare", SOURCE
                )
                module = importlib.util.module_from_spec(spec)
                spec.loader.exec_module(module)

                def dashboard(data, colors, wallpaper, artwork, home, monitors):
                    return invoke(
                        "lock_publish",
                        data=data,
                        colors=colors,
                        wallpaper=str(wallpaper),
                        artwork=str(artwork),
                        home=str(home),
                        monitors=monitors,
                        now="2023-11-14T22:13:20+00:00",
                        system_info=dict(
                            os="Fixture Linux", user="User", uptime="1h 00m"
                        ),
                    )

                def path(value, power=power):
                    return (
                        power.parent
                        if str(value) == "/sys/class/power_supply"
                        else Path(value)
                    )

                with (
                    patch.object(module.info, "Path", side_effect=path),
                    patch.object(module.info.time, "time", return_value=1700000000.125),
                    patch.object(module.dashboard, "publish", side_effect=dashboard),
                ):
                    result = module.prepare(data, home)
            if fingerprints(Path(folder)) != provider_before:
                raise AssertionError(
                    "The helper modified its read-only font provider inputs"
                )
            cases.append(
                dict(
                    name=name,
                    inputs=inputs,
                    data=files.normalize(data),
                    result=result,
                    outputs=files.snapshot(),
                )
            )
    DEST.write_bytes(
        gzip.compress(
            json.dumps(
                dict(
                    source_sha256=SOURCE_SHA,
                    font_inputs=font_inputs,
                    clock=1700000000.125,
                    native_clock="2023-11-14T22:13:20+00:00",
                    system_info=dict(os="Fixture Linux", user="User", uptime="1h 00m"),
                    cases=cases,
                ),
                indent=2,
            ).encode(),
            mtime=0,
        )
    )
    print(f"Frozen {len(cases)} original lock preparation cases and every file")


if __name__ == "__main__":
    main()
