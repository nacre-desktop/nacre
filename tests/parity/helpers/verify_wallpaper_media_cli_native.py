#!/usr/bin/env python3
"""Replay immutable media CLI/serial protocol responses and every state file."""

import gzip
import json
import os
from pathlib import Path
import subprocess
import tempfile

from freeze_wallpaper_media_cli import snapshot

from historical_palette_assets import stage as historical_assets

ROOT = Path(__file__).resolve().parents[3]


def main():
    reference = json.loads(
        gzip.decompress(
            (
                ROOT
                / "crates/nacre-helper/tests/fixtures/wallpaper-media-cli-reference.json.gz"
            ).read_bytes()
        )
    )
    binary = os.environ.get(
        "NACRE_HELPER_BINARY", str(ROOT / "target/debug/nacre-helper")
    )
    for case in reference["cases"]:
        with tempfile.TemporaryDirectory(prefix="nacre-media-cli-native-") as directory:
            home = Path(directory) / "home"
            home.mkdir(mode=0o700)
            env = {
                **os.environ,
                "HOME": str(home),
                "XDG_CONFIG_HOME": str(home / ".config"),
                "XDG_STATE_HOME": str(home / ".local/state"),
                "XDG_CACHE_HOME": str(home / ".cache"),
                "NACRE_HELPER_TOOLS": historical_assets(home.parent),
                "XDG_DATA_DIRS": str(home.parent / "empty-system-data"),
                "NACRE_LANGUAGE": "en",
            }
            for key in ("NACRE_PRESENTATION_STREAM", "NACRE_PALETTE_LOCKED"):
                env.pop(key, None)
            args = [word.replace("{home}", str(home)) for word in case["args"]]
            result = subprocess.run(
                [binary, "wallpaper-media", *args],
                env=env,
                input=case["input"],
                capture_output=True,
                text=True,
                timeout=30,
            )
            actual = dict(
                stdout=result.stdout.replace(str(home), "/fixture/home"),
                stderr=result.stderr.replace(str(home), "/fixture/home"),
                exit=result.returncode,
                after=snapshot(home),
            )
            expected = {key: case[key] for key in actual}
            if actual != expected:
                artifact = Path(
                    "/tmp/nacre-media-cli-" + case["name"] + "-difference.json"
                )
                artifact.write_text(
                    json.dumps(dict(actual=actual, expected=expected), indent=2)
                )
                raise AssertionError(
                    f"{case['name']}: differs; bounded artifact {artifact}"
                )
    print(
        f"Native media CLI/protocol: {len(reference['cases'])} immutable scenarios exact"
    )


if __name__ == "__main__":
    main()
