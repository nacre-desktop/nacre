#!/usr/bin/env python3
"""Freeze current display transactions, including rollback IPC and failures."""

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
    ROOT / "crates/nacre-helper/tests/fixtures/niri-monitor-transaction-reference.json"
)


def requests():
    base = dict(
        name="DP-2",
        width=2560,
        height=1440,
        scale=1.0,
        refreshRate=144.0,
        x=0,
        y=0,
        transform=0,
    )
    for name, records in (
        ("empty", []),
        ("single", [base]),
        (
            "pair",
            [
                base,
                {
                    **base,
                    "name": "DP-1",
                    "x": -1920,
                    "scale": 1.25,
                    "width": 1920,
                    "height": 1080,
                },
            ],
        ),
        ("no-refresh", [{**base, "refreshRate": 0}]),
        ("disconnected", [{**base, "name": "absent"}]),
        ("invalid-first", [{**base, "scale": 0}]),
    ):
        yield {
            "name": name,
            "method": "configure_monitors",
            "args": [records],
            "kwargs": {"timeout": 3},
        }
    for at in range(1, 5):
        for response in ("unexpected", "OutputWasMissing", "error"):
            yield {
                "name": f"failure-{at}-{response}",
                "method": "configure_monitors",
                "args": [[base]],
                "kwargs": {"timeout": 3},
                "failure_at": at,
                "failure": response,
            }
    for response in ("OutputWasMissing", "unexpected", "error"):
        yield {
            "name": "rollback-" + response,
            "method": "configure_monitors",
            "args": [[base]],
            "kwargs": {"timeout": 3},
            "failure_at": 2,
            "failure": "unexpected",
            "rollback": response,
        }


def child(request):
    state_reference.verify_source()
    spec = importlib.util.spec_from_file_location(
        "current_niri_monitor_transaction", state_reference.SOURCE
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
        index = len(calls)
        at = request.get("failure_at")
        response = (
            request.get("failure")
            if index == at
            else request.get("rollback", "Applied")
            if at and index > at
            else "Applied"
        )
        if response == "error":
            raise RuntimeError("fixture IPC failure")
        if response == "unexpected":
            return "unexpected"
        return {"OutputConfigChanged": response}

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
        raise ValueError(
            "Refusing to overwrite frozen niri monitor transaction outputs"
        )
    cases = []
    for request in requests():
        with tempfile.TemporaryDirectory(prefix="niri-monitor-reference-") as folder:
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
    print("Frozen niri monitor transaction cases:", capture(args.output))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
