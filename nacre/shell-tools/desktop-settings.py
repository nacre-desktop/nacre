#!/usr/bin/env python3
"""Validated desktop preferences and reversible connected-display arrangement."""

import nacre_native_compositor as compositor

import argparse
import fcntl
import json
import os
import subprocess
import sys
import time
import uuid
from pathlib import Path
from importlib.util import spec_from_file_location, module_from_spec

spec = spec_from_file_location("palette", Path(__file__).with_name("classic-state.py"))
palette = module_from_spec(spec)
spec.loader.exec_module(palette)
HOME = Path.home()
STATE = HOME / ".config/nacre/desktop.json"
LUA = compositor.config_path(HOME, "desktop")
PENDING = STATE.with_name(
    "display-pending-niri.json"
    if compositor.backend_name == "niri"
    else "display-pending.json"
)
DEFAULTS = json.loads(Path(__file__).with_name("desktop-defaults.json").read_text())
PRESET_KEYS = [
    key
    for key in DEFAULTS
    if key != "language" and not key.startswith(("lock", "weather", "interface"))
]
OPTIONS = set(compositor.desktop_option_keys())

RANGES = {
    "gapsIn": (0, 30),
    "gapsOut": (0, 80),
    "borderSize": (0, 8),
    "rounding": (0, 40),
    "frameWidth": (0, 30),
    "frameRounding": (0, 40),
}


def monitor_state():
    return [
        {
            k: m.get(k)
            for k in (
                "name",
                "width",
                "height",
                "refreshRate",
                "scale",
                "x",
                "y",
                "transform",
                "disabled",
                "mirrorOf",
                "availableModes",
            )
        }
        for m in compositor.query("all_monitors", timeout=8)
        if not m.get("disabled")
    ]


def initial():
    data = dict(DEFAULTS)
    for key in OPTIONS:
        try:
            data[key] = compositor.get_desktop_option(key, timeout=8)
        except (
            OSError,
            subprocess.SubprocessError,
            RuntimeError,
            ValueError,
            KeyError,
        ):
            pass
    return data


def load():
    data = (
        {**DEFAULTS, **json.loads(STATE.read_text())} if STATE.exists() else initial()
    )
    data.pop("wallpaperTransition", None)
    if compositor.capabilities.get("perOutputWorkspaces"):
        data["displays"] = data.get("compositorDisplays", {}).get(
            compositor.backend_name, []
        )
    if isinstance(data.get("normalSnapshot"), dict):
        data["normalSnapshot"].pop("wallpaperTransition", None)
    return data


def validate(key, value):
    if key not in DEFAULTS:
        raise ValueError("Unknown setting")
    if key in RANGES:
        low, high = RANGES[key]
        if type(value) is not int or not low <= value <= high:
            raise ValueError(f"{key} must be an integer from {low} to {high}")
    elif key == "language":
        if value not in ("system", "en", "pseudo") or (
            value == "pseudo" and os.environ.get("NACRE_DEVELOPER_MODE") != "1"
        ):
            raise ValueError("Choose System default or English")
    elif key in ("weatherLocation", "interfaceFont"):
        if (
            not isinstance(value, str)
            or len(value) > 80
            or any(ord(c) < 32 for c in value)
        ):
            raise ValueError("Use text up to80characters without control characters")
    elif type(value) is not bool:
        raise ValueError(f"{key} must be true or false")
    return value


def monitor_lua(monitors):
    return compositor.render_monitors(monitors)


def config_lua(data):
    return compositor.render_desktop_options(data)


def persist(data):
    if (
        compositor.capabilities.get("perOutputWorkspaces")
        and compositor.capabilities.get("monitorConfiguration")
        and not data.get("displays")
    ):
        data = {**data, "displays": monitor_state()}
    monitors = (
        []
        if compositor.capabilities.get("monitorConfiguration") is False
        else data.get("displays", [])
    )
    text = config_lua(data) + monitor_lua(monitors) + "\n"
    stored = dict(data)
    if compositor.capabilities.get("perOutputWorkspaces"):
        previous = json.loads(STATE.read_text()) if STATE.exists() else {}
        stored["compositorDisplays"] = {
            **previous.get("compositorDisplays", {}),
            compositor.backend_name: data.get("displays", []),
        }
        stored["displays"] = previous.get("displays", [])
    palette.atomic_write(STATE, json.dumps(stored, indent=2) + "\n")
    palette.atomic_write(LUA, text)


def ensure_include():
    compositor.ensure_desktop_loader(HOME)


def display_plan(monitors, primary, mode):
    if mode not in ("extend-right", "extend-left", "mirror"):
        raise ValueError("Unsupported display arrangement")
    if len(monitors) < 2:
        raise ValueError("Connect a second display first")
    main = next((m for m in monitors if m["name"] == primary), None)
    if not main:
        raise ValueError("Selected display is disconnected")
    main = dict(main, x=0, y=0, mirrorOf="none")
    result = [main]

    def width(m):
        return round(
            (m["height"] if (m.get("transform") or 0) % 2 else m["width"]) / m["scale"]
        )

    x = width(main) if mode == "extend-right" else 0
    for original in monitors:
        if original["name"] == primary:
            continue
        m = dict(original, y=0, mirrorOf=primary if mode == "mirror" else "none")
        if mode == "extend-left":
            x -= width(m)
        m["x"] = 0 if mode == "mirror" else x
        result.append(m)
        if mode == "extend-right":
            x += width(m)
    return result


def revert(token=None):
    if not PENDING.exists():
        return
    saved = json.loads(PENDING.read_text())
    if token and token != saved["token"]:
        return
    compositor.configure_monitors(saved["monitors"], timeout=8)
    data = load()
    data["displays"] = saved["previous"]
    persist(data)
    PENDING.unlink(missing_ok=True)


def state():
    data = load()
    return dict(
        data=data,
        monitors=monitor_state(),
        pending=PENDING.exists(),
        message="Changes save automatically",
    )


def main():
    p = argparse.ArgumentParser()
    p.add_argument(
        "action",
        choices=[
            "state",
            "init",
            "set",
            "display",
            "display-edit",
            "confirm",
            "revert",
            "rollback-after",
            "preset",
            "save-workflow",
        ],
    )
    p.add_argument("args", nargs="*")
    a = p.parse_args()
    if a.action == "rollback-after":
        time.sleep(20)
    STATE.parent.mkdir(parents=True, exist_ok=True)
    with STATE.with_suffix(".lock").open("w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        try:
            if a.action == "init":
                if PENDING.exists():
                    revert()
                persist(load())
                ensure_include()
            elif a.action == "set":
                key, raw = a.args
                if (
                    key
                    in {
                        "animations",
                        "blur",
                        "shadow",
                        "followMouse",
                        "naturalScroll",
                        "gapsIn",
                        "gapsOut",
                        "borderSize",
                        "rounding",
                    }
                    and key not in OPTIONS
                ):
                    raise ValueError(
                        "This compositor setting is unavailable; configure it in the compositor instead"
                    )
                value = validate(key, json.loads(raw))
                data = load()
                data[key] = value
                if key in OPTIONS:
                    compositor.set_desktop_options({key: value}, timeout=8)
                persist(data)
                if key.startswith("lock"):
                    spec = spec_from_file_location(
                        "lockconfig", Path(__file__).with_name("lock-config.py")
                    )
                    module = module_from_spec(spec)
                    spec.loader.exec_module(module)
                    module.publish(HOME)
            elif a.action == "preset":
                name = a.args[0]
                if name not in (
                    "normal",
                    "focused",
                    "presentation",
                    "minimal",
                    "meeting",
                    "music",
                    "docked",
                ):
                    raise ValueError("Unknown desktop preset")
                data = load()
                baseline = (
                    data.get("normalSnapshot")
                    if data.get("preset", "normal") != "normal"
                    else {k: data[k] for k in PRESET_KEYS}
                )
                baseline = {
                    k: v for k, v in (baseline or DEFAULTS).items() if k in PRESET_KEYS
                }
                overrides = {
                    "normal": {},
                    "focused": dict(
                        blur=False, shadow=False, animations=False, dnd=True
                    ),
                    "presentation": dict(
                        leftDrawer=False,
                        topEdge=False,
                        leftEdge=False,
                        rightEdge=False,
                        bottomEdge=False,
                        dnd=True,
                    ),
                    "meeting": dict(dnd=True, blur=False),
                    "music": dict(dnd=True),
                    "docked": dict(dnd=False),
                    "minimal": dict(
                        topEdge=True,
                        clickEdgeMenus=False,
                        leftEdge=False,
                        rightEdge=False,
                        bottomEdge=False,
                        frameWidth=0,
                        gapsIn=3,
                        gapsOut=6,
                        borderSize=0,
                        shadow=False,
                    ),
                }
                data.update(baseline)
                data.update(overrides[name])
                data["preset"] = name
                data["normalSnapshot"] = baseline
                compositor.set_desktop_options(data, timeout=8)
                persist(data)
                import importlib.util

                spec = importlib.util.spec_from_file_location(
                    "workflows", Path(__file__).with_name("workflow-profiles.py")
                )
                workflow = importlib.util.module_from_spec(spec)
                spec.loader.exec_module(workflow)
                workflow.HOME = HOME
                workflow.PATH = HOME / ".config/nacre/workflows.json"
                workflow.apply(name)
            elif a.action == "save-workflow":
                import importlib.util

                spec = importlib.util.spec_from_file_location(
                    "workflows", Path(__file__).with_name("workflow-profiles.py")
                )
                workflow = importlib.util.module_from_spec(spec)
                spec.loader.exec_module(workflow)
                workflow.save(
                    a.args[0], json.loads(a.args[1]) if len(a.args) > 1 else []
                )
            elif a.action in ("display", "display-edit"):
                if PENDING.exists():
                    raise ValueError("Keep or revert the current display change first")
                data = load()
                monitors = monitor_state()
                if a.action == "display":
                    mode, primary = a.args
                    if (
                        mode == "mirror"
                        and compositor.capabilities.get("monitorMirroring") is False
                    ):
                        raise ValueError(
                            "Output mirroring is unavailable on this compositor"
                        )
                    plan = display_plan(monitors, primary, mode)
                else:
                    name, scale, mode = a.args
                    scale = float(scale)
                    if scale not in (1, 1.25, 1.5, 1.75, 2, 2.5, 3):
                        raise ValueError("Unsupported display scale")
                    plan = [dict(m) for m in monitors]
                    selected = next((m for m in plan if m["name"] == name), None)
                    if selected is None:
                        raise ValueError("Selected display is disconnected")
                    if mode:
                        if mode not in selected.get("availableModes", []):
                            raise ValueError("Unavailable display mode")
                        import re

                        match = re.fullmatch(r"(\d+)x(\d+)@(\d+(?:\.\d+)?)Hz", mode)
                        if not match:
                            raise ValueError("Unsupported mode format")
                        selected.update(
                            width=int(match[1]),
                            height=int(match[2]),
                            refreshRate=float(match[3]),
                        )
                    selected["scale"] = scale
                token = uuid.uuid4().hex
                saved = dict(
                    token=token, previous=data.get("displays", []), monitors=monitors
                )
                palette.atomic_write(PENDING, json.dumps(saved))
                data["displays"] = plan
                rollback_environment = []
                rollback_script = __file__
                if compositor.capabilities.get("perOutputWorkspaces"):
                    rollback_environment.append(
                        "--setenv=NACRE_COMPOSITOR=" + compositor.backend_name
                    )
                    for name in (
                        "NACRE_COMPOSITOR",
                        "NIRI_SOCKET",
                        "NACRE_NIRI_CONFIG",
                        "NACRE_NIRI_DESKTOP_CONFIG",
                    ):
                        if name != "NACRE_COMPOSITOR" and os.environ.get(name):
                            rollback_environment.append(
                                "--setenv=" + name + "=" + os.environ[name]
                            )
                    if os.environ.get("NACRE_NIRI_TOOLS_SOURCE"):
                        rollback_script = str(
                            Path(os.environ["NACRE_NIRI_TOOLS_SOURCE"])
                            / "desktop-settings.py"
                        )
                rollback_worker = subprocess.Popen(
                    [
                        "systemd-run",
                        "--user",
                        "--collect",
                        "--quiet",
                        "--unit=nacre-display-revert-" + token,
                        *rollback_environment,
                        sys.executable,
                        rollback_script,
                        "rollback-after",
                        token,
                    ],
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    start_new_session=True,
                )
                if compositor.capabilities.get("perOutputWorkspaces"):
                    try:
                        if rollback_worker.wait(timeout=5) != 0:
                            raise RuntimeError(
                                "Automatic display rollback could not be armed"
                            )
                    except (RuntimeError, subprocess.SubprocessError):
                        PENDING.unlink(missing_ok=True)
                        raise
                try:
                    compositor.configure_monitors(plan, timeout=8)
                    persist(data)
                except Exception:  # noqa: BLE001 — Restore pending display transaction or report failure to the settings client.
                    revert(token)
                    raise
            elif a.action == "confirm":
                PENDING.unlink(missing_ok=True)
            elif a.action in ("revert", "rollback-after"):
                revert(a.args[0] if a.args else None)
            print(json.dumps(state()))
        except Exception as error:  # noqa: BLE001 — Restore pending display transaction or report failure to the settings client.
            print(
                json.dumps(
                    dict(data=load(), error=str(error), pending=PENDING.exists())
                )
            )


if __name__ == "__main__":
    main()
