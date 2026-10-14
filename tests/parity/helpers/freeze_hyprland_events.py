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
DESTINATION = ROOT / "crates/nacre-helper/tests/fixtures/hyprland-events-reference.json"


SOURCE = ROOT / "nacre/shell-tools/compositor_backends/hyprland.py"
SOURCE_SHA = "e0c253121be18b66cbc6e149fefb00157ff6b6f6d427dde49135ef499d363859"


def verify_source():
    if (
        not SOURCE.is_file()
        or hashlib.sha256(SOURCE.read_bytes()).hexdigest() != SOURCE_SHA
    ):
        raise ValueError("Hyprland reference source is absent or changed")


def requests():
    snapshot = {
        "monitors": [{"name": "DP-1"}],
        "workspaces": [{"id": 1}],
        "windows": [{"windowId": "0xabc"}],
        "active_window": {"windowId": "0xabc"},
    }
    for name, lines in (
        ("empty", []),
        (
            "events",
            [
                "activewindowv2>>abc",
                "windowtitlev2>>abc,Title",
                "monitoradded>>DP-2",
                "configreloaded>>",
            ],
        ),
        ("unknown", ["future>>data"]),
        ("no-final-newline", ["closewindow>>abc"]),
    ):
        yield {
            "name": name,
            "snapshot": snapshot,
            "lines": lines,
            "trailing_newline": name != "no-final-newline",
        }


def child(request):
    verify_source()
    spec = importlib.util.spec_from_file_location("current_hyprland_events", SOURCE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    import socket
    from unittest.mock import patch

    client, server = socket.socketpair()
    wire = "\n".join(request["lines"]) + (
        "\n" if request["lines"] and request["trailing_newline"] else ""
    )
    server.sendall(wire.encode())
    server.close()
    backend = module.Backend()
    queries = []

    def query(name):
        queries.append(name)
        return request["snapshot"][name]

    with (
        patch.object(backend, "event_socket", return_value=client),
        patch.object(backend, "query", side_effect=query),
    ):
        emitted = [json.dumps(event) + "\n" for event in backend.events()]
    print(
        json.dumps(
            {
                "emitted": emitted,
                "queries": queries,
                "connection_closed": client.fileno() == -1,
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
    print("Frozen Hyprland stream cases:", capture(args.output))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
