#!/usr/bin/env python3
"""Freeze current settings validation and display geometry before the next port."""

import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "nacre/shell-tools/desktop-settings.py"
DEST = (
    ROOT
    / "crates/nacre-helper/tests/fixtures/desktop-settings-validation-reference.json"
)


def capture(function, *args):
    try:
        return dict(result=function(*args))
    except (ValueError, TypeError, KeyError) as error:
        return dict(error=str(error), type=type(error).__name__)


def main():
    if DEST.exists():
        raise ValueError("Refusing to replace frozen settings outputs")
    previous = dict(os.environ)
    with tempfile.TemporaryDirectory(prefix="desktop-settings-reference-") as folder:
        try:
            os.environ.update(
                HOME=folder,
                XDG_CONFIG_HOME=folder + "/.config",
                XDG_STATE_HOME=folder + "/.local/state",
                XDG_DATA_HOME=folder + "/.local/share",
                XDG_CACHE_HOME=folder + "/.cache",
                XDG_RUNTIME_DIR=folder + "/runtime",
                DBUS_SESSION_BUS_ADDRESS="unix:path=" + folder + "/absent-session",
                DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + folder + "/absent-system",
                NACRE_COMPOSITOR="hyprland",
                NACRE_DEVELOPER_MODE="0",
                HYPRLAND_INSTANCE_SIGNATURE="fixture",
                NACRE_HELPER_BINARY=str(ROOT / "target/release/nacre-helper"),
            )
            spec = importlib.util.spec_from_file_location(
                "original_desktop_settings", SOURCE
            )
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            inputs = [(key, value) for key, value in module.DEFAULTS.items()]
            inputs += [
                (key, value)
                for key, values in (
                    ("unknown", [True]),
                    ("mouseSpeed", [True, -1, 1, -1.01, 1.01, "0.5", None]),
                    ("touchpadSpeed", [False, -1, 1, 2]),
                    ("mouseAcceleration", ["flat", "other", None]),
                    ("keyRepeatRate", [True, 0, 1, 80, 81, 1.0]),
                    ("language", ["system", "en", "pseudo", "unknown"]),
                    (
                        "weatherLocation",
                        ["", "A" * 80, "A" * 81, "line\nbreak", "x\x7f", None],
                    ),
                    ("interfaceFont", ["Fixture Sans", True]),
                    ("monoFont", ["Fixture Mono", "A" * 81]),
                    ("animations", [False, 0, "false", None]),
                )
                for value in values
            ]
            for key, (low, high) in module.RANGES.items():
                inputs += [
                    (key, value)
                    for value in (low - 1, low, high, high + 1, True, float(low))
                ]
            validation = [
                dict(key=key, value=value, **capture(module.validate, key, value))
                for key, value in inputs
            ]
            os.environ["NACRE_DEVELOPER_MODE"] = "1"
            developer = capture(module.validate, "language", "pseudo")
            os.environ.pop("NACRE_DEVELOPER_MODE")
            screens = [
                dict(name="DP-1", width=2560, height=1440, scale=1.25, transform=0),
                dict(name="DP-2", width=2880, height=1800, scale=1.5, transform=1),
                dict(name="DP-3", width=1920, height=1080, scale=1, transform=0),
            ]
            plans = [
                dict(
                    monitors=monitors,
                    primary=primary,
                    mode=mode,
                    **capture(module.display_plan, monitors, primary, mode),
                )
                for monitors, primary, mode in [
                    *[
                        (screens, "DP-1", mode)
                        for mode in ("extend-right", "extend-left", "mirror")
                    ],
                    (screens, "DP-2", "extend-right"),
                    (screens[:1], "DP-1", "extend-right"),
                    (screens, "missing", "extend-left"),
                    (screens, "DP-1", "other"),
                ]
            ]
            # Saved data load is independent of live compositor options.
            module.compositor = SimpleNamespace(
                capabilities=dict(perOutputWorkspaces=True), backend_name="niri"
            )
            module.STATE.parent.mkdir(parents=True)
            saved = dict(
                frameWidth=8,
                wallpaperTransition="old",
                normalSnapshot=dict(wallpaperTransition="old", gapsOut=27),
                displays=[dict(name="legacy")],
                compositorDisplays=dict(niri=[dict(name="winit")]),
            )
            module.STATE.write_text(json.dumps(saved))
            loaded = module.load()
            DEST.write_text(
                json.dumps(
                    dict(
                        source_sha256=hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
                        defaults=module.DEFAULTS,
                        inputKeys=module.INPUT_KEYS,
                        presetKeys=module.PRESET_KEYS,
                        validation=validation,
                        developerPseudo=developer,
                        plans=plans,
                        load=dict(saved=saved, result=loaded),
                    ),
                    indent=2,
                )
                + "\n"
            )
            print(
                "Frozen",
                len(validation),
                "validations;",
                len(plans),
                "display plans; saved state/defaults",
            )
        finally:
            os.environ.clear()
            os.environ.update(previous)


if __name__ == "__main__":
    main()
