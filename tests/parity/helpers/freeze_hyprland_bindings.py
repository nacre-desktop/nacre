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
    ROOT / "crates/nacre-helper/tests/fixtures/hyprland-bindings-reference.json"
)


SOURCE = ROOT / "nacre/shell-tools/compositor_backends/hyprland.py"
SOURCE_SHA = "e0c253121be18b66cbc6e149fefb00157ff6b6f6d427dde49135ef499d363859"


def verify_source():
    if (
        not SOURCE.is_file()
        or hashlib.sha256(SOURCE.read_bytes()).hexdigest() != SOURCE_SHA
    ):
        raise ValueError("Hyprland reference source is absent or changed")


LUA_SHA = "a9b7f893aee4b4f9fd1744851cdecd43b6aec2ff0066cfd714a8787ed87f133d"


def requests():
    text = 'hl.bind("SUPER+Q", hl.dsp.window.close(), {description="Close"})\nhl.define_submap("test", function() hl.bind("A", hl.dsp.exec_cmd("printf hello")) end)\nhl.window_rule({match={class="Example"},floating=true})\n'
    for kind in ("bindings", "rules", "declarations", "bad"):
        yield {"name": kind, "kind": kind, "text": text}
    yield {"name": "empty", "kind": "bindings", "text": "-- empty\n"}
    yield {"name": "lua-missing", "kind": "bindings", "text": text, "missing": True}


def child(request):
    verify_source()
    lua_source = SOURCE.with_name("hyprland") / "bindings.lua"
    if hashlib.sha256(lua_source.read_bytes()).hexdigest() != LUA_SHA:
        raise ValueError("Lua capture source changed")
    spec = importlib.util.spec_from_file_location("current_hyprland_bindings", SOURCE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    backend = module.Backend()
    target = Path.home() / "bindings.lua"
    target.write_text(request["text"])
    from unittest.mock import patch

    with patch.object(
        module.shutil,
        "which",
        return_value=None if request.get("missing") else "/usr/bin/lua",
    ):
        try:
            outcome, code = backend.capture_bindings(target, request["kind"]), 0
        except (ValueError, RuntimeError) as error:
            outcome, code = {"error": str(error), "type": type(error).__name__}, 1
    target.unlink()
    print(json.dumps({"stdout": json.dumps(outcome) + "\n", "exit": code}))
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
                    "lua_sha256": LUA_SHA,
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
    print("Frozen Hyprland binding cases:", capture(args.output))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
