#!/usr/bin/env python3
"""Freeze night-light policy with synthetic process records and no live services."""

import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
from unittest.mock import patch

import freeze_niri_state as state_reference

ROOT = state_reference.ROOT
DESTINATION = (
    ROOT / "crates/nacre-helper/tests/fixtures/niri-night-light-reference.json"
)


def requests():
    for method in ("night_light_command", "set_night_light_temperature"):
        for enabled in (True, False):
            yield {
                "name": method + "-" + str(enabled),
                "method": method,
                "args": [enabled],
                "available": True,
            }
    yield {
        "name": "temperature-unavailable",
        "method": "set_night_light_temperature",
        "args": [True],
        "available": False,
    }
    for name, tools, stdout, code, raises in (
        ("available", True, "zwlr_gamma_control_manager_v1", 0, False),
        ("no-tools", False, "", 0, False),
        ("no-gamma", True, "wl_compositor", 0, False),
        ("probe-error", True, "", 1, False),
        ("probe-exception", True, "", 0, True),
    ):
        yield {
            "name": name,
            "method": "night_light_available",
            "args": [],
            "tools": tools,
            "probe_stdout": stdout,
            "probe_code": code,
            "probe_raises": raises,
        }
    for config in ("", "gammastep/config.ini", "wlsunset/config"):
        yield {
            "name": "config-" + config,
            "method": "night_light_configured_elsewhere",
            "args": [],
            "config": config,
        }
    for name, active, own, processes in (
        ("no-controller", False, "", []),
        (
            "owned-controller",
            True,
            "123",
            [{"pid": "123", "command": "wlsunset", "display": "fixture-0"}],
        ),
        (
            "external-controller",
            True,
            "123",
            [{"pid": "456", "command": "gammastep", "display": "fixture-0"}],
        ),
        (
            "other-display",
            False,
            "",
            [{"pid": "456", "command": "wlsunset", "display": "other"}],
        ),
        (
            "other-command",
            False,
            "",
            [{"pid": "456", "command": "unrelated", "display": "fixture-0"}],
        ),
    ):
        yield {
            "name": name,
            "method": "night_light_external_socket",
            "args": [active],
            "own": own,
            "processes": processes,
        }


def child(request):
    state_reference.verify_source()
    spec = importlib.util.spec_from_file_location(
        "current_niri_night_light", state_reference.SOURCE
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    backend = module.Backend()
    calls = []
    home = Path.home()
    config = request.get("config", "")
    if config:
        path = home / ".config" / config
        path.parent.mkdir(parents=True)
        path.write_text("fixture")
    process_root = home / "proc"
    process_root.mkdir()
    for record in request.get("processes", []):
        folder = process_root / record["pid"]
        folder.mkdir()
        folder.joinpath("cmdline").write_bytes(record["command"].encode() + b"\0")
        folder.joinpath("environ").write_bytes(
            (
                "WAYLAND_DISPLAY="
                + record["display"]
                + "\0XDG_RUNTIME_DIR=/case/runtime\0"
            ).encode()
        )
    original_glob = Path.glob

    def glob(path, pattern):
        return (
            original_glob(process_root, pattern)
            if path == Path("/proc")
            else original_glob(path, pattern)
        )

    def run(command, **kwargs):
        calls.append({"command": command, "kwargs": kwargs})
        if request.get("probe_raises"):
            raise OSError("fixture probe failure")
        stdout = (
            request.get("own", "")
            if command[:1] == ["systemctl"] and "show" in command
            else request.get("probe_stdout", "")
        )
        return subprocess.CompletedProcess(
            command, request.get("probe_code", 0), stdout, ""
        )

    with (
        patch.dict(
            module.os.environ,
            {"WAYLAND_DISPLAY": "fixture-0", "XDG_RUNTIME_DIR": "/case/runtime"},
            clear=False,
        ),
        patch.object(
            module.shutil,
            "which",
            side_effect=lambda name: (
                "/case/bin/" + name if request.get("tools", True) else None
            ),
        ),
        patch.object(module.subprocess, "run", side_effect=run),
        patch.object(Path, "glob", glob),
    ):
        if request["method"] == "set_night_light_temperature":
            backend.night_light_available = lambda: request.get("available", True)
        try:
            args = (
                [home]
                if request["method"] == "night_light_configured_elsewhere"
                else request["args"]
            )
            result = getattr(backend, request["method"])(*args)
            if isinstance(result, subprocess.CompletedProcess):
                result = {
                    "code": result.returncode,
                    "stdout": result.stdout,
                    "stderr": result.stderr,
                }
            outcome, code = result, 0
        except (OSError, ValueError, RuntimeError) as error:
            outcome, code = {"error": str(error), "type": type(error).__name__}, 1
    print(
        json.dumps({"stdout": json.dumps(outcome) + "\n", "exit": code, "calls": calls})
    )
    # These are test inputs, removed before capture checks for helper writes.
    import shutil

    shutil.rmtree(process_root)
    if config:
        shutil.rmtree(home / ".config")
    return 0


def capture(destination):
    state_reference.verify_source()
    if destination.exists():
        raise ValueError("Refusing to overwrite frozen niri night-light outputs")
    cases = []
    for request in requests():
        with tempfile.TemporaryDirectory(
            prefix="niri-night-light-reference-"
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
    print("Frozen niri night-light cases:", capture(args.output))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
