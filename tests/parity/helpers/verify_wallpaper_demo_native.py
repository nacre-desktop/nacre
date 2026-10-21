#!/usr/bin/env python3
"""Replay original demo snapshot/entry/history bytes through native methods."""

import base64
import gzip
import json
import os
from pathlib import Path
import subprocess
import tempfile

from colors_reference_files import Files

from historical_palette_assets import stage as historical_assets

ROOT = Path(__file__).resolve().parents[3]


def main():
    reference = json.loads(
        gzip.decompress(
            (
                ROOT
                / "crates/nacre-helper/tests/fixtures/wallpaper-demo-reference.json.gz"
            ).read_bytes()
        )
    )
    for case in reference["cases"]:
        with tempfile.TemporaryDirectory(prefix="nacre-demo-native-") as folder:
            home = Path(folder) / "home"
            home.mkdir()
            for name, entry in case["inputs"].items():
                path = home / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(
                    entry["text"].replace("/fixture/home", str(home)).encode()
                    if "text" in entry
                    else base64.b64decode(entry["base64"])
                )
                path.chmod(entry["mode"])
                delta = (
                    int(path.stem, 16)
                    if name.startswith(".local/state/nacre/welcome/demo/")
                    else 0
                )
                os.utime(path, ns=(reference["input_mtime_ns"] + delta,) * 2)
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
            result = subprocess.run(
                [str(ROOT / "target/debug/examples/wallpaper-media-parity")],
                input=json.dumps(
                    dict(steps=[dict(action="demo-start", token=reference["token"])])
                ),
                env=env,
                capture_output=True,
                text=True,
                timeout=60,
            )
            assert result.returncode == 0, f"Native demo failed: {result.stderr[-500:]}"
            value = json.loads(result.stdout)[0]
            snapshot = json.loads(
                (
                    home
                    / ".local/state/nacre/welcome/demo"
                    / (reference["token"] + ".json")
                ).read_text()
            )
            files = Files(home, [])
            files.discover([value, snapshot["media"]])
            assert files.normalize(value) == case["result"], (
                f"{case['name']}: demo entries differ"
            )
            observed = files.snapshot()
            expected = case["outputs"]
            assert set(observed) == set(expected), f"{case['name']}: file set differs"
            for name in expected:
                if observed[name] != expected[name]:
                    Path("/tmp/nacre-demo-native-file-difference.json").write_text(
                        json.dumps(
                            dict(
                                path=name,
                                actual=observed[name],
                                expected=expected[name],
                            ),
                            indent=2,
                        )
                    )
                    raise AssertionError(f"{case['name']}: file differs {name}")
    print(f"Native demos: {len(reference['cases'])} snapshot/entry/history cases exact")


if __name__ == "__main__":
    main()
