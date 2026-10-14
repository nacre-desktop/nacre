#!/usr/bin/env python3
"""Freeze current pure Niri query/config outputs before native implementation."""

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
DESTINATION = (
    ROOT / "crates/nacre-helper/tests/fixtures/niri-config-query-reference.json"
)


def requests():
    def case(name, method, *args, **kwargs):
        return {"name": name, "method": method, "args": args, "kwargs": kwargs}

    for name in (
        "monitors",
        "all_monitors",
        "workspaces",
        "windows",
        "active_window",
        "active_workspace",
        "cursor",
        "instances",
        "unknown",
    ):
        yield case("query-" + name, "query", name, timeout=3, max_bytes=8192)
    for name in ("active_window", "cursor", "unknown"):
        for check in (True, False):
            yield case(
                "query-result-" + name + "-" + str(check),
                "query_result",
                name,
                check=check,
            )
    yield case("keyboard-empty", "render_keyboard", {})
    yield case(
        "keyboard-fields",
        "render_keyboard",
        {
            "layout": "us,tr",
            "model": "pc105",
            "variant": "",
            "options": "caps:escape",
            "rules": "evdev",
        },
    )
    yield case(
        "keyboard-unicode", "render_keyboard", {"layout": "tr", "options": 'quote"\\\n'}
    )
    yield case("keyboard-invalid", "render_keyboard", {"invalid": "us"})
    yield case("desktop-empty", "render_desktop_options", {})
    for enabled in (True, False):
        yield case(
            "desktop-all-" + str(enabled),
            "render_desktop_options",
            dict(
                animations=enabled,
                blur=enabled,
                shadow=enabled,
                followMouse=enabled,
                naturalScroll=enabled,
                gapsIn=6,
                borderSize=2,
                rounding=12,
            ),
        )
    for key in ("animations", "blur", "shadow", "followMouse", "naturalScroll"):
        for value in (True, False, "true", 1, None):
            yield case(
                "desktop-" + key + "-" + str(value),
                "render_desktop_options",
                {key: value},
            )
    for key in ("gapsIn", "borderSize", "rounding"):
        for value in (0, 100, 1.5, -1, 101, True, "1; off", None):
            yield case(
                "dimension-" + key + "-" + str(value),
                "render_desktop_options",
                {key: value},
            )
    yield case(
        "desktop-unknown-ignored", "render_desktop_options", {"unknown": "value"}
    )
    yield case("monitors-empty", "render_monitors", [])
    base = dict(
        name="DP-1", width=1920, height=1080, scale=1.0, refreshRate=60.0, x=-1920, y=0
    )
    for transform in range(8):
        yield case(
            "monitor-transform-" + str(transform),
            "render_monitors",
            [{**base, "transform": transform}],
        )
    for key, value in (
        ("name", ""),
        ("name", "bad\n"),
        ("mirrorOf", "DP-2"),
        ("disabled", True),
        ("scale", 0),
        ("scale", True),
        ("width", 32769),
        ("refreshRate", 1001),
        ("transform", 8),
    ):
        yield case(
            "monitor-invalid-" + key + "-" + str(value),
            "render_monitors",
            [{**base, key: value}],
        )
    yield case(
        "monitors-pair",
        "render_monitors",
        [base, {**base, "name": "eDP-1", "x": 0, "scale": 1.25, "refreshRate": 59.95}],
    )
    for value in (
        "niri-window:0",
        "niri-window:18446744073709551615",
        "niri-window:01",
        "bad",
        True,
        None,
    ):
        yield case("valid-id-" + str(value), "valid_window_id", value)
        yield case("job-token-" + str(value), "window_job_token", value)
    for kind in ("palette", "desktop", "monitor", "profile", "bad"):
        yield case("path-" + kind, "config_path", "/case/home", kind)
    yield case("legacy-power", "legacy_power_binding")
    yield case("unavailable-option", "get_desktop_option", "blur")


def child(request):
    state_reference.verify_source()
    spec = importlib.util.spec_from_file_location(
        "current_niri_config_query", state_reference.SOURCE
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
    print("Frozen niri config/query cases:", capture(args.output))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
