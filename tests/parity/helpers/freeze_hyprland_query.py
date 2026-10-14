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
DESTINATION = ROOT / "crates/nacre-helper/tests/fixtures/hyprland-query-reference.json"


SOURCE = ROOT / "nacre/shell-tools/compositor_backends/hyprland.py"
SOURCE_SHA = "e0c253121be18b66cbc6e149fefb00157ff6b6f6d427dde49135ef499d363859"


def verify_source():
    if (
        not SOURCE.is_file()
        or hashlib.sha256(SOURCE.read_bytes()).hexdigest() != SOURCE_SHA
    ):
        raise ValueError("Hyprland reference source is absent or changed")


def requests():
    row = {
        "address": "0xabc",
        "class": "Example",
        "title": "Title",
        "workspace": {"id": 7},
        "monitor": 2,
        "at": [-30, 40],
        "size": [800, 600],
        "futureField": "retained",
        "hidden": False,
        "pinned": True,
        "fullscreen": 2,
    }
    for name, response in (
        ("monitors", [{"name": "DP-1"}]),
        ("all_monitors", []),
        ("workspaces", [{"id": 7}]),
        ("windows", [row]),
        ("active_window", row),
        ("active_workspace", {"id": 7}),
        ("cursor", {"x": -30, "y": 40}),
        ("instances", [{"instance": "fixture"}]),
        ("bindings", [{"key": "G", "modmask": 64}]),
    ):
        yield {
            "name": "valid-" + name,
            "query": name,
            "reply": json.dumps(response),
            "max_bytes": None,
        }
        yield {
            "name": "wrong-shape-" + name,
            "query": name,
            "reply": "null",
            "max_bytes": None,
        }
    yield {
        "name": "no-active-window",
        "query": "active_window",
        "reply": "{}",
        "max_bytes": None,
    }
    yield {"name": "unknown", "query": "unknown", "reply": "[]", "max_bytes": None}
    yield {
        "name": "bad-json",
        "query": "windows",
        "reply": "invalid",
        "max_bytes": None,
    }
    yield {"name": "bad-record", "query": "windows", "reply": "[1]", "max_bytes": None}
    yield {
        "name": "size-bound",
        "query": "windows",
        "reply": json.dumps([row]),
        "max_bytes": 8,
    }
    yield {
        "name": "missing-geometry-defaults",
        "query": "windows",
        "reply": json.dumps([{"address": "0x1"}]),
        "max_bytes": None,
    }
    yield {
        "name": "bad-geometry",
        "query": "windows",
        "reply": json.dumps([{**row, "at": []}]),
        "max_bytes": None,
    }


def child(request):
    verify_source()
    spec = importlib.util.spec_from_file_location("current_hyprland_query", SOURCE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    backend = module.Backend()
    calls = []

    def run(args, **kwargs):
        calls.append({"args": args, "kwargs": kwargs})
        return subprocess.CompletedProcess([], 0, request["reply"], "")

    backend._run = run
    try:
        outcome, code = (
            backend.query(request["query"], timeout=3, max_bytes=request["max_bytes"]),
            0,
        )
    except (ValueError, RuntimeError, KeyError, TypeError, IndexError) as error:
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
    print("Frozen Hyprland query cases:", capture(args.output))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
