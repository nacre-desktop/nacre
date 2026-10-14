#!/usr/bin/env python3
"""Freeze CURRENT Python desktop writes, validation order and rollback files."""

import argparse
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
from unittest.mock import patch

import freeze_niri_state as reference

DESTINATION = (
    reference.ROOT
    / "crates/nacre-helper/tests/fixtures/niri-desktop-transaction-reference.json"
)


def requests():
    for name, changes in (
        ("new", {}),
        ("existing", {"previous": "// Existing user configuration\n"}),
        (
            "stored-options",
            {"state": {"gapsIn": 8, "rounding": 12, "unrelated": "keep"}},
        ),
        ("stored-monitors", {"stored_monitors": True}),
        ("missing-loader", {"loader": None}),
        ("wrong-loader", {"loader": "// No managed include\n"}),
        ("invalid-toggle", {"preferences": {"blur": "true"}}),
        ("invalid-dimension", {"preferences": {"gapsIn": -1}}),
        ("invalid-state", {"state_text": "invalid"}),
        ("validation-failure-new", {"validation_failure": True}),
        (
            "validation-failure-existing",
            {"validation_failure": True, "previous": "// Original\n"},
        ),
        ("reload-failure-new", {"reload_failure": True}),
        (
            "reload-failure-existing",
            {"reload_failure": True, "previous": "// Original\n"},
        ),
    ):
        yield {"name": name, "preferences": {"blur": True, "gapsIn": 6}, **changes}


def files(home):
    return {
        str(path.relative_to(home)): {
            "hex": path.read_bytes().hex(),
            "mode": oct(path.stat().st_mode & 0o777),
        }
        for path in sorted(home.rglob("*"))
        if path.is_file()
    }


def child(request):
    reference.verify_source()
    spec = importlib.util.spec_from_file_location(
        "current_niri_desktop_transaction", reference.SOURCE
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    home = Path.home()
    folder = home / ".config/nacre/niri"
    folder.mkdir(parents=True)
    loader = request.get("loader", 'include "desktop.kdl"\n')
    if loader is not None:
        (folder / "config.kdl").write_text(loader)
    target = folder / "desktop.kdl"
    if "previous" in request:
        target.write_text(request["previous"])
    fixture = json.loads(reference.FIXTURE.read_text())
    model = module.State(fixture["outputs"])
    for event in fixture["events"][:2]:
        model.apply(event)
    monitors = model.snapshot()["monitors"]
    state = copy.deepcopy(request.get("state", {}))
    if request.get("stored_monitors"):
        state["compositorDisplays"] = {"niri": monitors}
    if state or "state_text" in request:
        (folder.parent / "desktop.json").write_text(
            request.get("state_text", json.dumps(state))
        )
    initial = files(home)
    backend = module.Backend()
    calls = []

    def query(name, **kwargs):
        calls.append({"kind": "query", "name": name, "kwargs": kwargs})
        return copy.deepcopy(monitors)

    def validate(command, **kwargs):
        path = Path(command[-1])
        calls.append(
            {"kind": "validate", "bytes": path.read_bytes().hex(), "kwargs": kwargs}
        )
        if request.get("validation_failure"):
            raise RuntimeError("fixture validation failed")
        return subprocess.CompletedProcess(command, 0, "", "")

    def reload(**kwargs):
        calls.append(
            {"kind": "reload", "bytes": target.read_bytes().hex(), "kwargs": kwargs}
        )
        if request.get("reload_failure"):
            raise RuntimeError("fixture reload failed")
        return subprocess.CompletedProcess([], 0, "Handled", "")

    with (
        patch.object(backend, "query", side_effect=query),
        patch.object(backend, "reload", side_effect=reload),
        patch.object(module.subprocess, "run", side_effect=validate),
    ):
        try:
            result = backend.set_desktop_options(request["preferences"], timeout=3)
            outcome = {
                "code": result.returncode,
                "stdout": result.stdout,
                "stderr": result.stderr,
            }
            code = 0
        except (ValueError, RuntimeError, KeyError, TypeError, OSError) as error:
            outcome, code = {"error": str(error), "type": type(error).__name__}, 1
    print(
        json.dumps(
            {
                "stdout": json.dumps(outcome) + "\n",
                "exit": code,
                "calls": calls,
                "initial_files": initial,
                "files": files(home),
            }
        )
    )


def capture(destination):
    reference.verify_source()
    if destination.exists():
        raise ValueError(
            "Refusing to overwrite frozen Niri desktop transaction outputs"
        )
    cases = []
    for request in requests():
        with tempfile.TemporaryDirectory(prefix="niri-desktop-reference-") as folder:
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
            if result.returncode or result.stderr:
                raise ValueError(
                    "Unexpected desktop transaction capture failure: "
                    + request["name"]
                    + result.stderr[-500:]
                )
            observed = json.loads(result.stdout)
            if observed["files"] != files(Path(folder)):
                raise ValueError("Desktop capture omitted a file effect")
            cases.append({"request": request, "observed": observed, "stderr": ""})
    reference.verify_source()
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("x") as output:
        output.write(
            json.dumps(
                {
                    "source_sha256": reference.SOURCE_SHA,
                    "fixture_sha256": reference.FIXTURE_SHA,
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
        child(json.loads(args.child))
        return 0
    print("Frozen desktop transactions:", capture(args.output))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
