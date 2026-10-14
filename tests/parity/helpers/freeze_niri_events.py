#!/usr/bin/env python3
"""Freeze current Python event emission using closed private socket pairs."""

import argparse
import copy
import importlib.util
import json
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
from unittest.mock import patch

import freeze_niri_state as reference

DESTINATION = (
    reference.ROOT / "crates/nacre-helper/tests/fixtures/niri-events-reference.json"
)


def requests():
    fixture = json.loads(reference.FIXTURE.read_text())
    events = fixture["events"]
    outputs = fixture["outputs"]
    changed = copy.deepcopy(outputs)
    changed["DP-2"]["logical"]["x"] = 200
    updated = copy.deepcopy(events[1]["WindowsChanged"]["windows"][0])
    updated.update(title="Updated", workspace_id=202, is_floating=True)
    for name, selected, metadata in (
        ("complete-stream", events, [outputs]),
        ("initial-readiness", events[:2], [outputs]),
        ("workspace-first-only", events[:1], [outputs]),
        ("windows-first", [events[1], events[0]], [outputs]),
        (
            "outputs-changed-on-config",
            [*events[:2], {"ConfigLoaded": {"failed": False}}],
            [outputs, outputs, changed],
        ),
        (
            "multi-field-window-change",
            [*events[:2], {"WindowOpenedOrChanged": {"window": updated}}],
            [outputs],
        ),
        (
            "hover-lifetime",
            [
                *events[:2],
                {"OverviewOpenedOrClosed": {"is_open": True}},
                {"NacreOverviewHoverChanged": {"id": 9007199254740995}},
                {"OverviewOpenedOrClosed": {"is_open": False}},
            ],
            [outputs],
        ),
        ("unknown-event-silent", [*events[:2], {"FutureEvent": None}], [outputs]),
        ("malformed-event", [*events[:2], {"WindowClosed": None}], [outputs]),
    ):
        yield {"name": name, "events": selected, "outputs": metadata}
    for name, tail in (("truncated", '{"WindowClosed":'), ("bad-json", "invalid\n")):
        yield {"name": name, "events": events[:2], "outputs": [outputs], "tail": tail}


def child(request):
    reference.verify_source()
    spec = importlib.util.spec_from_file_location(
        "current_niri_events", reference.SOURCE
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    client, server = socket.socketpair()
    data = b"".join((json.dumps(event) + "\n").encode() for event in request["events"])
    data += request.get("tail", "").encode()
    server.sendall(data)
    server.close()
    backend = module.Backend()
    queries = []

    def outputs(name, **kwargs):
        queries.append({"name": name, "kwargs": kwargs})
        return copy.deepcopy(
            request["outputs"][min(len(queries) - 1, len(request["outputs"]) - 1)]
        )

    emitted = []
    with (
        patch.object(backend, "event_socket", return_value=client),
        patch.object(backend, "_response", side_effect=outputs),
    ):
        try:
            for event in backend.events():
                emitted.append(json.dumps(event, allow_nan=False) + "\n")
        except (ValueError, RuntimeError, OSError, KeyError, TypeError) as error:
            failure = {"type": type(error).__name__, "error": str(error)}
        else:
            raise RuntimeError("A closed source stream did not report disconnection")
    print(
        json.dumps(
            {
                "emitted": emitted,
                "error": failure,
                "queries": queries,
                "connection_closed": client.fileno() == -1,
            }
        )
    )
    return 0


def capture(destination):
    reference.verify_source()
    if destination.exists():
        raise ValueError("Refusing to overwrite frozen niri event outputs")
    cases = []
    for request in requests():
        with tempfile.TemporaryDirectory(prefix="niri-events-reference-") as folder:
            env = {
                "HOME": folder,
                "PATH": "",
                "PYTHONDONTWRITEBYTECODE": "1",
                "XDG_CONFIG_HOME": folder + "/.config",
                "XDG_STATE_HOME": folder + "/.local/state",
                "XDG_CACHE_HOME": folder + "/.cache",
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
                timeout=5,
                check=False,
            )
            if result.returncode or result.stderr or list(Path(folder).rglob("*")):
                raise ValueError(
                    "Unexpected niri event capture effect: " + request["name"]
                )
            cases.append(
                {
                    "request": request,
                    "observed": json.loads(result.stdout),
                    "stderr": "",
                    "files": {},
                }
            )
    reference.verify_source()
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("x") as output:
        output.write(
            json.dumps(
                {"source_sha256": reference.SOURCE_SHA, "cases": cases}, indent=2
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
    print("Frozen niri event streams:", capture(args.output))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
