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
DESTINATION = ROOT / "crates/nacre-helper/tests/fixtures/hyprland-files-reference.json"


SOURCE = ROOT / "nacre/shell-tools/compositor_backends/hyprland.py"
SOURCE_SHA = "e0c253121be18b66cbc6e149fefb00157ff6b6f6d427dde49135ef499d363859"


def verify_source():
    if (
        not SOURCE.is_file()
        or hashlib.sha256(SOURCE.read_bytes()).hexdigest() != SOURCE_SHA
    ):
        raise ValueError("Hyprland reference source is absent or changed")


def requests():
    for name, text in (
        ("valid", '-- Nacre desktop settings\nload_private("desktop")\n'),
        ("missing-marker", 'load_private("desktop")\n'),
        ("missing-loader", "-- Nacre desktop settings\n"),
    ):
        yield {
            "name": name,
            "method": "ensure_desktop_loader",
            "files": {".config/hypr/hyprland.lua": text},
        }
    yield {
        "name": "binding-files",
        "method": "binding_source_files",
        "files": {
            ".config/hypr/conf/a.lua": 'hl.bind("Q")',
            ".config/hypr/conf/b.lua": "-- no binding",
            ".config/nacre/shortcuts.lua": 'hl.bind ("G")',
        },
    }
    for present in (True, False):
        yield {
            "name": "night-config-" + str(present),
            "method": "night_light_configured_elsewhere",
            "files": {".config/hypr/hyprsunset.conf": "fixture"} if present else {},
        }
    yield {"name": "legacy-power", "method": "legacy_power_binding", "files": {}}


def child(request):
    verify_source()
    spec = importlib.util.spec_from_file_location("current_hyprland_files", SOURCE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    backend = module.Backend()
    home = Path.home()
    for name, content in request["files"].items():
        path = home / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
    try:
        result = (
            getattr(backend, request["method"])(home)
            if request["method"] != "legacy_power_binding"
            else backend.legacy_power_binding()
        )
        if isinstance(result, list):
            result = [str(path.relative_to(home)) for path in result]
        outcome, code = result, 0
    except (ValueError, RuntimeError, OSError) as error:
        outcome, code = {"error": str(error), "type": type(error).__name__}, 1
    print(json.dumps({"stdout": json.dumps(outcome) + "\n", "exit": code}))
    import shutil

    if (home / ".config").exists():
        shutil.rmtree(home / ".config")
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
    print("Frozen Hyprland file cases:", capture(args.output))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
