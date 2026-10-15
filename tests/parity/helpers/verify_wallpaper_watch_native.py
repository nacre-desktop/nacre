#!/usr/bin/env python3
"""Replay immutable watcher process/module/filesystem and event contracts."""

import json
import os
from pathlib import Path
import subprocess
import tempfile

from wallpaper_watch_reference import capture

ROOT = Path(__file__).resolve().parents[3]
FIXTURES = ROOT / "crates/nacre-helper/tests/fixtures"


def main():
    binary = Path(
        os.environ.get("NACRE_HELPER_BINARY", ROOT / "target/debug/nacre-helper")
    )
    api = Path(
        os.environ.get("NACRE_PARITY_API", ROOT / "target/debug/examples/parity-api")
    )
    reference = json.loads((FIXTURES / "wallpaper-watch-reference.json").read_text())
    options = json.loads(
        (FIXTURES / "wallpaper-watch-cli-options-reference.json").read_text()
    )
    cli = reference["cli"] + options["cases"]
    for case in cli:
        with tempfile.TemporaryDirectory(prefix="nacre-watch-cli-native-") as home:
            result = subprocess.run(
                [str(binary), "wallpaper-watch", *case["args"]],
                env={**os.environ, "HOME": home},
                capture_output=True,
                text=True,
            )
            assert (result.stdout, result.stderr, result.returncode) == (
                case["stdout"],
                case["stderr"],
                case["exit"],
            ), f"CLI mismatch: {case['args']}"
            assert not list(Path(home).rglob("*")), (
                "CLI help/errors must not write files"
            )
    actual = capture([str(binary), "wallpaper-watch"])
    if actual != reference["dialogue"]:
        artifact = Path("/tmp/nacre-wallpaper-watch-difference.json")
        artifact.write_text(
            json.dumps(dict(actual=actual, expected=reference["dialogue"]), indent=2)
        )
        fields = [
            key for key in actual if actual[key] != reference["dialogue"].get(key)
        ]
        raise AssertionError(
            f"Watcher dialogue mismatch in {fields}; details {artifact}"
        )
    module = json.loads(
        (FIXTURES / "wallpaper-watch-flavour-reference.json").read_text()
    )
    for case in module["cases"]:
        with tempfile.TemporaryDirectory(prefix="nacre-watch-module-native-") as home:
            path = Path(home) / "scheme.json"
            if case["input"] is not None:
                path.write_text(case["input"])
            request = dict(args=[{"$path": str(path)}], kwargs={})
            result = subprocess.run(
                [str(api), "wallpaper-watch", "flavour", json.dumps(request)],
                env={**os.environ, "HOME": home},
                capture_output=True,
                text=True,
            )
            assert (result.stdout, result.returncode) == (
                case["stdout"],
                case["exit"],
            ), f"Module mismatch: {case['name']}"
    print(
        f"Watcher: {len(cli)} CLI, {len(actual['steps'])} event steps, {len(module['cases'])} module cases and every file match"
    )


if __name__ == "__main__":
    main()
