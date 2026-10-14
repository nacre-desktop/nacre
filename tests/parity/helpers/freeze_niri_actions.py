#!/usr/bin/env python3
"""Freeze current semantic action results and exact niri IPC requests."""

import argparse
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile

import freeze_niri_state as state_reference

ROOT = state_reference.ROOT
DESTINATION = ROOT / "crates/nacre-helper/tests/fixtures/niri-actions-reference.json"


def requests():
    def case(name, method, *args, **kwargs):
        return {"name": name, "method": method, "args": args, "kwargs": kwargs}

    for index, value in enumerate(
        (
            1,
            "1",
            {"output": "DP-1", "index": 1},
            {"id": "niri-workspace:202"},
            {"name": "coding"},
            {"relative": 1},
            {"relative": -1},
            {"relative": True},
            True,
            0,
            "missing",
            "niri-workspace:01",
        )
    ):
        yield case("workspace-" + str(index), "switch_workspace", value, timeout=3)
    for direction in ("left", "right", "up", "down", "unknown"):
        yield case("focus-" + direction, "focus_direction", direction, timeout=3)
        yield case("move-" + direction, "move_direction", direction, timeout=3)
    for direction in ("next", "previous", "unknown"):
        yield case("cycle-" + direction, "cycle_window", direction)
    for method in ("focus_window", "close_window"):
        for window in ("", "niri-window:1", "niri-window:9007199254740995", "bad"):
            yield case(method + "-" + window, method, window, timeout=3)
    for action in ("toggle", "enable", "disable", "set", "unset", "bad"):
        yield case("float-" + action, "float_window", "niri-window:1", action=action)
    for mode in ("fullscreen", "maximized", "bad"):
        yield case(
            "fullscreen-" + mode, "fullscreen_window", "niri-window:1", mode=mode
        )
    for index, (width, height, relative) in enumerate(
        (
            (600, 400, False),
            (600.5, 401.5, False),
            (-5.5, 2.5, True),
            (0, 400, False),
            (True, 400, False),
        )
    ):
        yield case(
            "resize-" + str(index),
            "resize_window",
            "niri-window:1",
            width,
            height,
            relative=relative,
            timeout=3,
        )
    yield case(
        "screenshot-absolute",
        "screenshot_window",
        "/case/home/Pictures/window.png",
        "niri-window:1",
    )
    yield case(
        "screenshot-relative", "screenshot_window", "window.png", "niri-window:1"
    )
    yield case(
        "move-workspace", "move_window", "niri-window:1", workspace=2, follow=False
    )
    yield case(
        "move-workspace-geometry-conflict",
        "move_window",
        "niri-window:1",
        workspace=2,
        x=10,
        y=20,
    )
    yield case(
        "move-relative",
        "move_window",
        "niri-window:9007199254740995",
        x=10,
        y=-20,
        relative=True,
    )
    yield case(
        "move-absolute", "move_window", "niri-window:9007199254740995", x=100, y=200
    )
    value = case(
        "move-cross-output-requery",
        "move_window",
        "niri-window:9007199254740995",
        x=-1800,
        y=100,
    )
    value["move_output"] = "DP-1"
    yield value
    yield case("move-tiled-rejected", "move_window", "niri-window:1", x=10, y=20)
    yield case(
        "place",
        "place_window",
        "niri-window:9007199254740995",
        x=100,
        y=200,
        width=500,
        height=400,
        timeout=3,
    )
    yield case(
        "place-missing-height", "place_window", "niri-window:1", x=100, y=200, width=500
    )
    yield case("move-workspace-to-monitor", "move_workspace", 1, "DP-1")
    yield case("move-workspace-missing-monitor", "move_workspace", 1, "missing")
    for action in ("enable", "disable", "on", "off", "bad"):
        yield case("dpms-" + action, "dpms", action)
    yield case("dpms-per-monitor-rejected", "dpms", "off", monitor="DP-1")
    yield case("logout", "logout", graceful=True)
    yield case("reload", "reload")
    yield case("toggle-workspace-floating", "toggle_workspace_floating")
    for method in (
        "pin_window",
        "kill_window",
        "set_cursor",
        "notify",
        "fullscreen_state",
        "set_animations",
        "set_performance",
        "set_options",
        "render_options",
        "apply_config",
    ):
        yield case("unsupported-" + method, method, "fixture")


def child(request):
    state_reference.verify_source()
    spec = importlib.util.spec_from_file_location(
        "current_niri_actions", state_reference.SOURCE
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    fixture = json.loads(state_reference.FIXTURE.read_text())
    model = module.State(fixture["outputs"])
    for event in fixture["events"][:2]:
        model.apply(event)
    snapshot = model.snapshot()
    probes = []
    calls = []
    backend = module.Backend()

    def state(timeout=10, max_bytes=module.MAX_BYTES):
        probes.append({"timeout": timeout, "max_bytes": max_bytes})
        return copy.deepcopy(snapshot)

    def transport(value, *, timeout=10, max_bytes=module.MAX_BYTES):
        calls.append(
            {
                "request": copy.deepcopy(value),
                "timeout": timeout,
                "max_bytes": max_bytes,
            }
        )
        if request.get("move_output") and "MoveWindowToMonitor" in value.get(
            "Action", {}
        ):
            for window in snapshot["windows"]:
                if window["windowId"] == "niri-window:9007199254740995":
                    window["outputId"] = "DP-1"
                    window["geometry"].update(x=-1892, y=44)
            snapshot["activeWindow"]["outputId"] = "DP-1"
            snapshot["activeWindow"]["geometry"].update(x=-1892, y=44)
        return "Handled"

    backend._state = state
    backend._request = transport
    try:
        result = getattr(backend, request["method"])(
            *request["args"], **request["kwargs"]
        )
        if isinstance(result, subprocess.CompletedProcess):
            result = {
                "code": result.returncode,
                "stdout": result.stdout,
                "stderr": result.stderr,
            }
        outcome, code = result, 0
    except (ValueError, RuntimeError, KeyError, TypeError) as error:
        outcome, code = {"error": str(error), "type": type(error).__name__}, 1
    print(
        json.dumps(
            {
                "stdout": json.dumps(outcome, allow_nan=False) + "\n",
                "exit": code,
                "probes": probes,
                "requests": calls,
            },
            allow_nan=False,
        )
    )
    return 0


def capture(destination):
    state_reference.verify_source()
    if destination.exists():
        raise ValueError("Refusing to overwrite frozen niri action outputs")
    cases = []
    for request in requests():
        with tempfile.TemporaryDirectory(prefix="niri-actions-reference-") as folder:
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
    state_reference.verify_source()
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("x") as output:
        output.write(
            json.dumps(
                {
                    "source_sha256": state_reference.SOURCE_SHA,
                    "fixture_sha256": state_reference.FIXTURE_SHA,
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
    print("Frozen niri action cases:", capture(args.output))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
