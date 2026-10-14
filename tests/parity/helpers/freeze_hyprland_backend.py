#!/usr/bin/env python3
"""Freeze current Hyprland semantic actions and pure rendering before porting."""

import argparse
import importlib.util
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

import freeze_niri_state as state_reference

ROOT = state_reference.ROOT
DESTINATION = (
    ROOT / "crates/nacre-helper/tests/fixtures/hyprland-backend-reference.json"
)


SOURCE = ROOT / "nacre/shell-tools/compositor_backends/hyprland.py"
SOURCE_SHA = "e0c253121be18b66cbc6e149fefb00157ff6b6f6d427dde49135ef499d363859"


def verify_source():
    if (
        not SOURCE.is_file()
        or hashlib.sha256(SOURCE.read_bytes()).hexdigest() != SOURCE_SHA
    ):
        raise ValueError("Hyprland reference source is absent or changed")


def requests():
    def case(name, method, *args, **kwargs):
        return {"name": name, "method": method, "args": args, "kwargs": kwargs}

    for value in (
        1,
        "name:code",
        {"id": 3},
        {"name": "code"},
        {"output": "DP-1", "index": 1},
        True,
        None,
    ):
        yield case("workspace-" + str(value), "switch_workspace", value, timeout=3)
    for method in ("focus_direction", "move_direction", "cycle_window"):
        for direction in ("left", "right", "up", "down", "next", "previous", "bad"):
            yield case(method + "-" + direction, method, direction)
    for method in (
        "focus_window",
        "close_window",
        "kill_window",
        "float_window",
        "pin_window",
        "fullscreen_window",
    ):
        for window in (
            "",
            "0xabc",
            "class:Example",
            "title:Title",
            "pid:123",
            "active",
            "bad",
            "0xabc;exit",
        ):
            yield case(method + "-" + window, method, window, timeout=3)
    yield case("move-workspace", "move_window", "0xabc", workspace=2, follow=False)
    yield case("move-geometry", "move_window", "0xabc", x=-30, y=40, relative=True)
    yield case("move-workspace-monitor", "move_workspace", 3, "DP-2")
    yield case("resize", "resize_window", "0xabc", 600, 400, relative=True)
    yield case(
        "place", "place_window", "0xabc", x=-30, y=40, width=600, height=400, timeout=3
    )
    for action in ("on", "off", "toggle"):
        yield case("dpms-" + action, "dpms", action, monitor="DP-2")
    yield case(
        "fullscreen-state",
        "fullscreen_state",
        "0xabc",
        internal=2,
        client=1,
        action="set",
    )
    yield case("cursor", "set_cursor", "Example", 24)
    yield case("notify", "notify", 1, 5000, "0xff00ff", "Message")
    yield case(
        "keyboard", "render_keyboard", {"layout": "us,tr", "options": "caps:escape"}
    )
    yield case(
        "options",
        "render_options",
        {"general": {"gaps_in": 6}, "quoted-key": 'quote"\n'},
    )
    yield case(
        "desktop",
        "render_desktop_options",
        {
            "animations": False,
            "blur": True,
            "followMouse": True,
            "gapsIn": 6,
            "rounding": 12,
        },
    )
    yield case("monitors-empty", "render_monitors", [])
    yield case(
        "monitor-record",
        "render_monitors",
        [
            {
                "name": "DP-1",
                "width": 1920,
                "height": 1080,
                "refreshRate": 59.95,
                "scale": 1.25,
                "x": -1920,
                "y": 0,
                "transform": 0,
            }
        ],
    )
    yield case(
        "monitor-native",
        "render_monitors",
        [{"output": "DP-1", "mode": "preferred", "scale": 1.0, "position": "auto"}],
    )
    for value in (True, False):
        yield case("performance-" + str(value), "set_performance", value)
        yield case("animations-" + str(value), "set_animations", value)
    yield case("reload", "reload")
    yield case("toggle-floating", "toggle_workspace_floating")
    for line in (
        "activewindowv2>>abc",
        "openwindow>>abc,1,Class,Title",
        "closewindow>>0xabc",
        "windowtitlev2>>abc,Title",
        "fullscreen>>1",
        "monitoraddedv2>>DP-1",
        "workspacev2>>1,code",
        "future>>data",
        "urgent>>bad;id",
        "configreloaded>>",
    ):
        yield case("event-" + line, "parse_event", line)
    for kind in ("palette", "desktop", "monitor", "profile", "bad"):
        yield case("path-" + kind, "config_path", "/case/home", kind)
    yield case("capabilities", "capabilities")


def child(request):
    verify_source()
    spec = importlib.util.spec_from_file_location("current_hyprland", SOURCE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    backend = module.Backend()
    calls = []

    def run(args, **kwargs):
        calls.append({"args": args, "kwargs": kwargs})
        return subprocess.CompletedProcess([], 0, "ok\n", "")

    backend._run = run
    try:
        result = (
            backend.capabilities
            if request["method"] == "capabilities"
            else getattr(backend, request["method"])(
                *request["args"], **request["kwargs"]
            )
        )
        if isinstance(result, subprocess.CompletedProcess):
            result = {
                "code": result.returncode,
                "stdout": result.stdout,
                "stderr": result.stderr,
            }
        if isinstance(result, Path):
            result = str(result)
        outcome, code = result, 0
    except (ValueError, RuntimeError, KeyError, TypeError) as error:
        outcome, code = {"error": str(error), "type": type(error).__name__}, 1
    print(
        json.dumps(
            {
                "stdout": json.dumps(outcome, allow_nan=False) + "\n",
                "exit": code,
                "calls": calls,
            }
        )
    )
    return 0


def capture(destination):
    verify_source()
    if destination.exists():
        raise ValueError("Refusing to overwrite frozen niri config/query outputs")
    cases = []
    for request in requests():
        with tempfile.TemporaryDirectory(
            prefix="niri-config-query-reference-"
        ) as folder:
            home = Path(folder)
            env = {
                "HOME": folder,
                "PATH": "",
                "PYTHONDONTWRITEBYTECODE": "1",
                "XDG_CONFIG_HOME": str(home / ".config"),
                "XDG_STATE_HOME": str(home / ".local/state"),
                "XDG_CACHE_HOME": str(home / ".cache"),
            }
            result = subprocess.run(
                [
                    sys.executable,
                    str(Path(__file__).resolve()),
                    "--child",
                    json.dumps(request),
                ],
                env=env,
                stdin=subprocess.DEVNULL,
                capture_output=True,
                text=True,
                timeout=10,
                check=False,
            )
            if result.returncode or result.stderr or list(home.rglob("*")):
                raise ValueError("Unexpected action capture effect: " + request["name"])
            cases.append(
                {
                    "request": request,
                    "observed": json.loads(result.stdout),
                    "stderr": "",
                    "files": {},
                }
            )
    verify_source()
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("x") as output:
        output.write(
            json.dumps(
                {
                    "source_sha256": SOURCE_SHA,
                    "cases": cases,
                },
                indent=2,
            )
            + "\n"
        )
    return len(cases)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=DESTINATION)
    parser.add_argument("--child")
    args = parser.parse_args()
    if args.child:
        return child(json.loads(args.child))
    print("Frozen Hyprland backend cases:", capture(args.output))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
