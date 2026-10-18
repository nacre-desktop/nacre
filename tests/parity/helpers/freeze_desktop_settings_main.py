#!/usr/bin/env python3
"""Freeze the current settings controller with inert hardware/timer providers."""

import contextlib
import copy
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
from unittest.mock import patch

from run import snapshot

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "nacre/shell-tools/desktop-settings.py"
DEST = (
    ROOT
    / "crates/nacre-helper/tests/fixtures/desktop-settings-controller-reference.json"
)


def capture(backend, arguments, failure=""):
    previous = dict(os.environ)
    with tempfile.TemporaryDirectory(prefix="dsref-") as folder:
        home = Path(folder)
        try:
            for name in tuple(os.environ):
                if name.startswith("NACRE_NIRI_"):
                    os.environ.pop(name)
            os.environ.update(
                HOME=folder,
                XDG_CONFIG_HOME=folder + "/.config",
                XDG_STATE_HOME=folder + "/.local/state",
                XDG_CACHE_HOME=folder + "/.cache",
                XDG_DATA_HOME=folder + "/.local/share",
                XDG_RUNTIME_DIR=folder + "/runtime",
                DBUS_SESSION_BUS_ADDRESS="unix:path=" + folder + "/absent-session",
                DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + folder + "/absent-system",
                NACRE_COMPOSITOR=backend,
                NIRI_SOCKET=folder + "/runtime/niri.sock",
                NIRI_CONFIG=folder + "/.config/niri/config.kdl",
                HYPRLAND_INSTANCE_SIGNATURE="fixture",
                WAYLAND_DISPLAY="wayland-test",
                XDG_CURRENT_DESKTOP=backend,
                NACRE_HELPER_TOOLS=str(ROOT / "nacre/shell-tools"),
                NACRE_DEVELOPER_MODE="0",
                NACRE_HELPER_BINARY=str(ROOT / "target/release/nacre-helper"),
            )
            spec = importlib.util.spec_from_file_location(
                "original_settings_controller", SOURCE
            )
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            native = module.compositor
            module.PENDING = module.STATE.with_name(
                "display-pending-niri.json"
                if backend == "niri"
                else "display-pending.json"
            )
            module.LUA = module.STATE.with_name(
                "desktop-niri.kdl" if backend == "niri" else "desktop.lua"
            )
            module.STATE.parent.mkdir(parents=True)
            saved = {
                key: value
                for key, value in module.DEFAULTS.items()
                if key not in module.INPUT_KEYS
            }
            saved.update(
                gapsOut=27,
                lockMedia=False,
                lockWeather=False,
                lockNotifications=False,
                lockNotificationContents=True,
                weatherLocation="Fixture city",
                weatherFahrenheit=True,
            )
            if failure == "mono-font-state":
                saved["monoFont"] = "Old Mono"
            module.STATE.write_text(json.dumps(saved))
            monitors = [
                dict(
                    name=name,
                    width=width,
                    height=height,
                    refreshRate=120,
                    scale=scale,
                    x=0,
                    y=0,
                    transform=0,
                    disabled=False,
                    mirrorOf="none",
                    availableModes=[f"{width}x{height}@120Hz"],
                )
                for name, width, height, scale in [
                    ("DP-1", 2560, 1440, 1.25),
                    ("DP-2", 1920, 1080, 1),
                ]
            ]
            effects = []

            def effect(name, *args, **kwargs):
                effects.append(
                    copy.deepcopy(dict(operation=name, args=args, kwargs=kwargs))
                )

            module.compositor = SimpleNamespace(
                backend_name=backend,
                capabilities=dict(
                    perOutputWorkspaces=backend == "niri",
                    monitorConfiguration=True,
                    monitorMirroring=backend != "niri",
                ),
                query=lambda *_, **__: copy.deepcopy(monitors),
                render_desktop_options=lambda data: native.render_for_backend(
                    backend, "render_desktop_options", data
                ),
                render_monitors=lambda data: native.render_for_backend(
                    backend, "render_monitors", data
                ),
                set_desktop_options=lambda *a, **k: effect(
                    "set_desktop_options", *a, **k
                ),
                configure_monitors=lambda *a, **k: effect(
                    "configure_monitors", *a, **k
                ),
                ensure_desktop_loader=lambda *a, **k: effect(
                    "ensure_desktop_loader", *a, **k
                ),
                apply_config=lambda *a, **k: effect("apply_config", *a, **k),
                reload=lambda *a, **k: effect("reload", *a, **k),
            )
            module.publish_mono_font = lambda family: effect("terminal-font", family)
            if arguments[0] in ("confirm", "revert", "rollback-after"):
                module.PENDING.write_text(
                    json.dumps(
                        dict(token="fixed-token", previous=[], monitors=monitors)
                    )
                )
            output = io.StringIO()
            original_write = module.palette.atomic_write
            failed = []

            def write(path, text, *args, **kwargs):
                if not failed and (
                    (failure == "mono-font-state" and path == module.STATE)
                    or (failure == "display-config" and path == module.LUA)
                ):
                    failed.append(True)
                    raise OSError("fixture write failure")
                return original_write(path, text, *args, **kwargs)

            original_popen = subprocess.Popen

            def popen(command, *args, **kwargs):
                if command[0] == "systemd-run":
                    effect("systemd-run", command)
                    return SimpleNamespace(wait=lambda **_: 0)
                return original_popen(command, *args, **kwargs)

            # Only hardware effects and the timer service are inert. Real storage,
            # validation, presets, renderers and control flow remain unchanged.
            with (
                patch.object(sys, "argv", ["desktop-settings.py", *arguments]),
                patch.object(
                    module.uuid,
                    "uuid4",
                    return_value=SimpleNamespace(
                        hex="0123456789abcdef0123456789abcdef"
                    ),
                ),
                patch.object(module.subprocess, "Popen", side_effect=popen),
                patch.object(module.palette, "atomic_write", side_effect=write),
                patch.object(
                    module,
                    "time",
                    SimpleNamespace(sleep=lambda seconds: effect("sleep", seconds)),
                ),
                contextlib.redirect_stdout(output),
            ):
                code = module.main()
            result = dict(
                backend=backend,
                environment={
                    key: os.environ[key]
                    for key in (
                        "HOME",
                        "XDG_CONFIG_HOME",
                        "XDG_STATE_HOME",
                        "XDG_CACHE_HOME",
                        "XDG_DATA_HOME",
                        "NIRI_SOCKET",
                        "NIRI_CONFIG",
                        "NACRE_COMPOSITOR",
                        "NACRE_HELPER_BINARY",
                    )
                },
                args=arguments,
                stdout=output.getvalue(),
                exit=code or 0,
                effects=effects,
                files=snapshot(home),
            )
            # Runtime roots and interpreter names are fixture inputs, not personal paths.
            text = (
                json.dumps(result, default=lambda item: {"$path": str(item)})
                .replace(folder, "{home}")
                .replace(str(ROOT), "{root}")
                .replace(sys.executable, "{python}")
            )
            return json.loads(text)
        finally:
            os.environ.clear()
            os.environ.update(previous)


def main():
    if DEST.exists():
        raise ValueError("Refusing to replace frozen settings outputs")
    cases = []
    for backend in ("hyprland", "niri"):
        for args in [
            ["state"],
            ["init"],
            ["set", "animations", "false"],
            ["set", "gapsOut", "10000"],
            ["set", "gapsOut", "true"],
            ["set", "unknown", "false"],
            ["set", "monoFont", '"Fixture Mono"'],
            ["set", "interfaceGradients", "false"],
            *[
                ["preset", name]
                for name in (
                    "focused",
                    "presentation",
                    "minimal",
                    "meeting",
                    "music",
                    "docked",
                    "normal",
                )
            ],
            ["display", "extend-right", "DP-1"],
            ["display", "mirror", "DP-1"],
            ["display-edit", "DP-1", "1.5", "2560x1440@120Hz"],
            ["display-edit", "missing", "1.5", ""],
            ["confirm"],
            ["revert", "wrong"],
            ["revert", "fixed-token"],
            ["rollback-after", "fixed-token"],
        ]:
            cases.append(capture(backend, args))
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
    print(
        "Frozen", len(cases), "settings controller contracts with all final files/modes"
    )


if __name__ == "__main__":
    main()
