#!/usr/bin/env python3
"""Replay immutable standalone palette CLI stdout/errors/files through Rust."""

import base64
import gzip
import json
import os
from pathlib import Path
import subprocess
import tempfile

from colors_reference_files import Files, unpack

from historical_palette_assets import stage as historical_assets

ROOT = Path(__file__).resolve().parents[3]


def main():
    packed = json.loads(
        gzip.decompress(
            (
                ROOT
                / "crates/nacre-helper/tests/fixtures/palette-cli-reference.json.gz"
            ).read_bytes()
        )
    )
    reference = unpack(packed["data"], packed["strings"])
    binary = Path(
        os.environ.get(
            "NACRE_PALETTE_CLI_PARITY",
            str(ROOT / "target/debug/examples/palette-cli-parity"),
        )
    )
    for case in reference["cases"]:
        with tempfile.TemporaryDirectory(prefix="nacre-cli-native-") as directory:
            home = Path(directory)
            for relative, entry in case["inputs"].items():
                path = home / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                if "link" in entry:
                    path.symlink_to(entry["link"].replace("/fixture/home", str(home)))
                else:
                    raw = (
                        entry["text"].replace("/fixture/home", str(home)).encode()
                        if "text" in entry
                        else base64.b64decode(entry["base64"])
                    )
                    path.write_bytes(raw)
                    path.chmod(entry["mode"])
                    os.utime(path, ns=(reference["input_mtime_ns"],) * 2)
            environment = {
                **os.environ,
                "HOME": str(home),
                "XDG_CONFIG_HOME": str(home / ".config"),
                "XDG_STATE_HOME": str(home / ".local/state"),
                "XDG_CACHE_HOME": str(home / ".cache"),
                "NACRE_HELPER_TOOLS": historical_assets(home.parent),
                "NACRE_LANGUAGE": "en",
                "NACRE_COMPOSITOR": "hyprland",
            }
            results = []
            for command in case["commands"]:
                args = [word.replace("{home}", str(home)) for word in command]
                worker = subprocess.run(
                    [str(binary), "python -m nacre_shell", *args],
                    env=environment,
                    text=True,
                    capture_output=True,
                    timeout=120,
                )
                results.append(
                    dict(
                        stdout=worker.stdout,
                        stderr=worker.stderr,
                        exit=worker.returncode,
                    )
                )
            files = Files(home, [])
            files.discover(results)
            actual = files.normalize(results)
            if actual != case["results"]:
                artifact = Path("/tmp") / (
                    "nacre-cli-" + case["name"] + "-difference.json"
                )
                artifact.write_text(
                    json.dumps(dict(actual=actual, expected=case["results"]), indent=2)
                )
                raise AssertionError(
                    f"{case['name']}: CLI result differs; bounded details {artifact}"
                )
            actual = files.snapshot()
            expected = case["outputs"]
            if set(actual) != set(expected):
                raise AssertionError(
                    f"{case['name']}: extra files{sorted(set(actual) - set(expected))[:5]}, missing{sorted(set(expected) - set(actual))[:5]}"
                )
            for name, value in expected.items():
                if actual[name] != value:
                    artifact = Path("/tmp") / (
                        "nacre-cli-" + case["name"] + "-file-difference.json"
                    )
                    artifact.write_text(
                        json.dumps(
                            dict(path=name, actual=actual[name], expected=value),
                            indent=2,
                        )
                    )
                    raise AssertionError(
                        f"{case['name']}: file differs {name}; details {artifact}"
                    )
            print(f"{case['name']}: exact stdout/exit and{len(expected)} files")
    print(f"Native palette CLI: {len(reference['cases'])} immutable scenarios passed")
    help_reference = json.loads(
        (
            ROOT / "crates/nacre-helper/tests/fixtures/palette-cli-help-reference.json"
        ).read_text()
    )
    for case in help_reference["cases"]:
        with tempfile.TemporaryDirectory(prefix="nacre-cli-help-native-") as directory:
            result = subprocess.run(
                [str(binary), "python -m nacre_shell", *case["args"]],
                env={**os.environ, "HOME": directory},
                capture_output=True,
                text=True,
                timeout=30,
            )
            assert (result.stdout, result.stderr, result.returncode) == (
                case["stdout"],
                case["stderr"],
                case["exit"],
            ), f"Help output differs: {case['args']}"
            assert not list(Path(directory).rglob("*")), "Help must not write state"
    print(
        f"Native palette help: {len(help_reference['cases'])} immutable scenarios passed"
    )


if __name__ == "__main__":
    main()
