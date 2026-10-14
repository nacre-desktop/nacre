#!/usr/bin/env python3
"""Freeze current pearl ownership, geometry and compositor calls before porting."""

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
DESTINATION = ROOT / "crates/nacre-helper/tests/fixtures/pearl-window-reference.json"


SOURCE = ROOT / "nacre/shell-tools/pearl-window.py"
SOURCE_SHA = "14119a141133775be482fb5068554a77a580ab03126f4f9edd644b7008491b6f"


def verify_source():
    if (
        not SOURCE.is_file()
        or hashlib.sha256(SOURCE.read_bytes()).hexdigest() != SOURCE_SHA
    ):
        raise ValueError("Pearl window reference source is absent or changed")


def requests():
    client = {
        "title": "Nacre Pearl Launcher 1",
        "pid": 45,
        "address": "0xabc",
        "windowId": "0xabc",
        "at": [100, 880],
        "size": [1360, 320],
        "monitor": 0,
        "fullscreen": 0,
        "floating": True,
    }
    monitors = [
        {
            "id": 0,
            "x": 0,
            "y": 0,
            "width": 2880,
            "height": 1800,
            "scale": 1.5,
            "reserved": [10, 50, 10, 10],
        }
    ]
    for action in ("geometry", "focus", "pin", "place", "fit"):
        yield {
            "name": action,
            "argv": [action, client["title"], "45", "100", "200", "640", "820"],
            "clients": [client],
            "monitors": monitors,
        }
    for name, changes in (
        ("maximized", {"fullscreen": 1}),
        ("no-floating", {"floating": False}),
        ("wrong-pid", {"pid": 46}),
    ):
        yield {
            "name": name,
            "argv": [
                "pin" if name == "no-floating" else "fit",
                client["title"],
                "45",
                "0",
                "0",
                "640",
                "820",
            ],
            "clients": [{**client, **changes}],
            "monitors": monitors,
        }
    yield {
        "name": "wrong-title",
        "argv": ["geometry", "unowned", "45"],
        "clients": [client],
        "monitors": monitors,
    }
    yield {
        "name": "negative-pid",
        "argv": ["geometry", client["title"], "-1"],
        "clients": [client],
        "monitors": monitors,
    }


def child(request):
    verify_source()
    sys.path.insert(0, str(ROOT / "nacre/shell-tools"))
    spec = importlib.util.spec_from_file_location("current_pearl_window", SOURCE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    from unittest.mock import patch
    import io
    from contextlib import redirect_stdout

    calls = []
    reads = []
    ticks = iter([0, 0, 1, 2, 3, 4, 5])

    def read(kind):
        reads.append(kind)
        return request["clients"] if kind == "clients" else request["monitors"]

    def dispatch(method, window, **kwargs):
        calls.append({"method": method, "window": window, "kwargs": kwargs})

    stdout = io.StringIO()
    with (
        patch.object(module, "read", side_effect=read),
        patch.object(module, "dispatch", side_effect=dispatch),
        patch.object(module.time, "monotonic", side_effect=lambda: next(ticks)),
        patch.object(module.time, "sleep"),
        patch.object(sys, "argv", ["pearl-window", *request["argv"]]),
        redirect_stdout(stdout),
    ):
        code = module.main()
    print(
        json.dumps(
            {"stdout": stdout.getvalue(), "exit": code, "calls": calls, "reads": reads}
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
                "NACRE_COMPOSITOR": "hyprland",
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
    print("Frozen pearl window cases:", capture(args.output))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
