#!/usr/bin/env python3
"""Freeze CURRENT Python niri event reductions and snapshots in disposable HOME."""

import argparse
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "nacre/shell-tools/compositor_backends/niri.py"
FIXTURE = ROOT / "nacre/shell-tools/tests/fixtures/niri-event-stream.json"
SOURCE_SHA = "b1befdc9049d11f53a3fdbf11eab038c5bd5f719f085c06371daecede4889847"
FIXTURE_SHA = "29ecb0b0a011aeb0562be71fe3793b6ab542c7d9e62e9351dbdb94113edbf0f0"
DESTINATION = ROOT / "crates/nacre-helper/tests/fixtures/niri-state-reference.json"


def verify_source():
    for path, expected in ((SOURCE, SOURCE_SHA), (FIXTURE, FIXTURE_SHA)):
        if (
            not path.is_file()
            or hashlib.sha256(path.read_bytes()).hexdigest() != expected
        ):
            raise ValueError("Niri state reference source is absent or changed")


def requests():
    fixture = json.loads(FIXTURE.read_text())
    initial = fixture["events"][:2]
    windows = fixture["events"][1]["WindowsChanged"]["windows"]
    first = windows[0]["id"]
    workspace = fixture["events"][0]["WorkspacesChanged"]["workspaces"][0]["id"]

    def case(name, events, outputs=None, prefix=True):
        return {
            "name": name,
            "outputs": fixture["outputs"] if outputs is None else outputs,
            "events": (initial if prefix else []) + events,
        }

    yield case("complete-reference-stream", fixture["events"], prefix=False)
    yield case("initial-u64-output-relative", [])
    yield case(
        "null-focus-and-stale-ids",
        [
            {"WindowFocusChanged": {"id": None}},
            {"WindowClosed": {"id": 2}},
            {"WindowUrgencyChanged": {"id": 2, "urgent": True}},
        ],
    )
    yield case(
        "overview-hover-lifetime",
        [
            {"OverviewOpenedOrClosed": {"is_open": True}},
            {"NacreOverviewHoverChanged": {"id": first}},
            {"OverviewOpenedOrClosed": {"is_open": False}},
            {"NacreOverviewHoverChanged": {"id": first}},
        ],
    )
    yield case(
        "reload-failure-until-success",
        [
            {"ConfigLoaded": {"failed": True}},
            {
                "WindowFocusTimestampChanged": {
                    "id": first,
                    "focus_timestamp": {"secs": 123, "nanos": 45},
                }
            },
            {"ConfigLoaded": {"failed": False}},
        ],
    )
    changed = copy.deepcopy(windows[0])
    changed.update(
        title="Changed fixture title",
        workspace_id=workspace,
        is_floating=not changed.get("is_floating", False),
    )
    yield case(
        "window-update-classification", [{"WindowOpenedOrChanged": {"window": changed}}]
    )
    yield case(
        "workspace-urgency-active-window",
        [
            {"WorkspaceUrgencyChanged": {"workspace_id": workspace, "urgent": True}},
            {
                "WorkspaceActiveWindowChanged": {
                    "id": workspace,
                    "active_window_id": first,
                }
            },
        ],
    )
    reordered = copy.deepcopy(fixture["events"][0])
    for row in reordered["WorkspacesChanged"]["workspaces"]:
        row["idx"] = 6 - row["idx"]
    yield case("workspace-reorder-stable-identity", [reordered])
    yield case("outputs-absent-never-invent-geometry", [], outputs={})
    disabled = copy.deepcopy(fixture["outputs"])
    disabled["DP-2"]["logical"] = None
    yield case("disabled-output-never-invent-geometry", [], outputs=disabled)
    moving = copy.deepcopy(windows[0])
    moving["workspace_id"] = None
    moving["layout"] = {"window_size": [500, 400], "tile_pos_in_workspace_view": None}
    yield case("interactive-move", [{"WindowOpenedOrChanged": {"window": moving}}])
    yield case(
        "unknown-future-events",
        [
            "FutureStringEvent",
            {"FutureNull": None},
            {"FutureArray": [1]},
            {"FutureNumber": 17},
        ],
    )
    yield case(
        "u64-max-identities",
        [
            {
                "WorkspacesChanged": {
                    "workspaces": [
                        {
                            "id": 2**64 - 1,
                            "idx": 1,
                            "output": "DP-1",
                            "is_active": True,
                            "is_focused": True,
                        }
                    ]
                }
            },
            {
                "WindowsChanged": {
                    "windows": [
                        {"id": 2**64 - 1, "workspace_id": 2**64 - 1, "is_focused": True}
                    ]
                }
            },
        ],
        prefix=False,
    )
    for name, event in (
        ("bad-event-array", []),
        ("bad-event-two-keys", {"A": {}, "B": {}}),
        ("bad-known-payload", {"WindowClosed": None}),
        ("bool-window-id", {"WindowClosed": {"id": True}}),
        ("overflow-window-id", {"WindowClosed": {"id": 2**64}}),
        ("missing-window-id", {"WindowClosed": {}}),
        ("bad-record-list", {"WindowsChanged": {"windows": [None]}}),
        ("focus-failure-after-mutation", {"WindowFocusChanged": {"id": True}}),
        (
            "layouts-failure-after-first-mutation",
            {
                "WindowLayoutsChanged": {
                    "changes": [[first, {"window_size": [700, 600]}], [True, {}]]
                }
            },
        ),
    ):
        yield case(name, [event])


def child(request):
    verify_source()
    spec = importlib.util.spec_from_file_location("current_niri_reference", SOURCE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    model = module.State(request["outputs"])
    observations = []
    for event in request["events"]:
        try:
            result = model.apply(event)
            code = 0
        except (ValueError, KeyError, TypeError, RuntimeError) as error:
            result = {"error": str(error), "type": type(error).__name__}
            code = 1
        observations.append(
            {
                "stdout": json.dumps(result, allow_nan=False) + "\n",
                "exit": code,
                "snapshot": model.snapshot(),
                "ready": sorted(model.ready),
            }
        )
    print(json.dumps(observations, allow_nan=False))
    return 0


def capture(destination):
    verify_source()
    if destination.exists():
        raise ValueError("Refusing to overwrite frozen niri state outputs")
    cases = []
    for request in requests():
        with tempfile.TemporaryDirectory(prefix="niri-state-reference-") as folder:
            home = Path(folder)
            environment = {
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
                env=environment,
                stdin=subprocess.DEVNULL,
                capture_output=True,
                text=True,
                timeout=10,
                check=False,
            )
            if result.returncode or result.stderr or list(home.rglob("*")):
                raise ValueError(
                    "Unexpected niri state capture effect: " + request["name"]
                )
            cases.append(
                {
                    "request": request,
                    "observations": json.loads(result.stdout),
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
                    "fixture_sha256": FIXTURE_SHA,
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
    print("Frozen niri state sequences:", capture(args.output))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
