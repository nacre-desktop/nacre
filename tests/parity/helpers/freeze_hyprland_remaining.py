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
    ROOT / "crates/nacre-helper/tests/fixtures/hyprland-remaining-reference.json"
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
    def case(name, method, *args, reply="ok\n", **kwargs):
        return {
            "name": name,
            "method": method,
            "args": args,
            "kwargs": kwargs,
            "reply": reply,
        }

    yield case("apply", "apply_config", "hl.config({})\n", timeout=3, check=False)
    yield case("set-options", "set_options", {"general": {"gaps_in": 6}}, timeout=3)
    yield case(
        "set-desktop",
        "set_desktop_options",
        {"blur": True, "followMouse": False},
        timeout=3,
    )
    yield case("configure-empty", "configure_monitors", [], timeout=3)
    yield case("config-errors", "config_errors", reply="  Invalid option\n", timeout=3)
    yield case("logout", "logout", timeout=3)
    for key in ("blur", "followMouse", "gapsIn"):
        for value in ({"bool": True}, {"int": 2}, {"css": "6 12 6 12"}):
            yield case(
                "option-" + key + "-" + str(value),
                "get_desktop_option",
                key,
                reply=json.dumps(value),
                timeout=3,
            )
    yield case("option-unknown", "get_desktop_option", "unknown", reply="{}")
    for value in (True, False):
        yield case("night-command-" + str(value), "night_light_command", value)
        yield case(
            "night-temperature-" + str(value),
            "set_night_light_temperature",
            value,
            timeout=1,
        )
    for value in ("0x0", "0xabc", "0xABC", "bad", True, None):
        yield case("valid-id-" + str(value), "valid_window_id", value)
        yield case("job-token-" + str(value), "window_job_token", value)
    for method in (
        "session_id",
        "version_command",
        "required_commands",
        "diagnostic_commands",
        "environment_updates",
    ):
        yield case(
            method,
            method,
            reply='[{"instance":"fixture"}]'
            if method == "environment_updates"
            else "ok\n",
        )
    yield case(
        "screenshot-unsupported", "screenshot_window", "/case/window.png", "0xabc"
    )


def child(request):
    verify_source()
    spec = importlib.util.spec_from_file_location("current_hyprland_remaining", SOURCE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    backend = module.Backend()
    calls = []

    def run(args, **kwargs):
        calls.append({"args": args, "kwargs": kwargs})
        return subprocess.CompletedProcess([], 0, request["reply"], "")

    backend._run = run
    from unittest.mock import patch

    with patch.dict(
        module.os.environ, {"HYPRLAND_INSTANCE_SIGNATURE": "fixture"}, clear=True
    ):
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
        json.dumps({"stdout": json.dumps(outcome) + "\n", "exit": code, "calls": calls})
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
    print("Frozen remaining Hyprland cases:", capture(args.output))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
