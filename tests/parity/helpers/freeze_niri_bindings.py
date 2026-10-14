#!/usr/bin/env python3
"""Freeze current pure Niri query/config outputs before native implementation."""

import argparse
import copy
import importlib.util
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

import freeze_niri_state as state_reference

ROOT = state_reference.ROOT
DESTINATION = ROOT / "crates/nacre-helper/tests/fixtures/niri-bindings-reference.json"


BINDINGS_SHA = "03fb7ddd6582124fcc7d37a6411d6b0eb26d19853851d02591876b32c70cbde4"


def verify_source():
    state_reference.verify_source()
    if (
        hashlib.sha256((ROOT / "niri/bindings.kdl").read_bytes()).hexdigest()
        != BINDINGS_SHA
    ):
        raise ValueError("Generated Niri binding reference source changed")


def requests():
    def case(name, text, method="capture_bindings", kind="bindings"):
        return {
            "name": name,
            "method": method,
            "args": [],
            "kwargs": {},
            "text": text,
            "kind": kind,
        }

    generated = (ROOT / "niri/bindings.kdl").read_text()
    yield case("generated-capture", generated)
    yield case("generated-query", generated, "query")
    prefix = "// Generated from compositor-contract.json; fixture\nbinds {\n"
    yield case(
        "keys-and-escapes",
        prefix
        + '    Super+Ctrl+Alt+Shift+Q repeat=false hotkey-overlay-title="Quote \\" and \\u00e7" { spawn-sh "printf \\"hello\\""; }\n}\n',
    )
    yield case(
        "native-action", prefix + "    Super+Q repeat=false { close-window; }\n}\n"
    )
    yield case(
        "ignored-lines", prefix + "    Super+Q { close-window; }\n  unknown syntax\n}\n"
    )
    yield case("private-override", "binds { Super+Q { close-window; } }")
    yield case("invalid-kind", generated, kind="rules")


def child(request):
    verify_source()
    spec = importlib.util.spec_from_file_location(
        "current_niri_bindings", state_reference.SOURCE
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
    target = Path.home() / "fixture-bindings.kdl"
    target.write_text(request["text"])
    backend.binding_source_files = lambda home: [target]
    request["args"] = (
        ["bindings"] if request["method"] == "query" else [target, request["kind"]]
    )
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
    target.unlink()
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
    verify_source()
    if destination.exists():
        raise ValueError("Refusing to overwrite frozen niri bindings outputs")
    cases = []
    for request in requests():
        with tempfile.TemporaryDirectory(prefix="niri-bindings-reference-") as folder:
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
                    "source_sha256": state_reference.SOURCE_SHA,
                    "fixture_sha256": state_reference.FIXTURE_SHA,
                    "bindings_sha256": BINDINGS_SHA,
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
    print("Frozen niri bindings cases:", capture(args.output))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
