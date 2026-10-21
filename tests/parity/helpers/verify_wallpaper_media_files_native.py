#!/usr/bin/env python3
"""Replay the immutable import/catalog workflow through the native importer."""

import base64
import gzip
import hashlib
import re
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

from colors_reference_files import Files
from freeze_wallpaper_media_files import WORKER

from historical_palette_assets import stage as historical_assets

ROOT = Path(__file__).resolve().parents[3]


def normalize_posters(value, aliases):
    """Only disposable-root/stat-derived dynamic poster and preview names vary."""
    if isinstance(value, str):
        for old, new in aliases.items():
            value = value.replace(old, new)
        return value
    if isinstance(value, list):
        return [normalize_posters(item, aliases) for item in value]
    if isinstance(value, dict):
        return {
            normalize_posters(key, aliases): normalize_posters(item, aliases)
            for key, item in value.items()
        }
    return value


def poster_aliases(value):
    aliases = {}

    def visit(item):
        if isinstance(item, dict):
            if item.get("dynamic") and item.get("poster") != item.get("path"):
                stem = Path(item["poster"]).stem
                assert re.fullmatch("[0-9a-f]{64}", stem)
                identity = hashlib.sha256(item["path"].encode()).hexdigest()[:16]
                aliases["/wallpaper-media/" + stem + ".png"] = (
                    "/wallpaper-media/@POSTER_" + identity + "@.png"
                )
                preview = Path(item["thumbnail"]).stem.removesuffix("-thumbnail")
                assert re.fullmatch("@MEDIA_[0-9a-f]{16}@", preview)
                aliases[preview] = "@MEDIA_" + identity + "@"
            for child in item.values():
                visit(child)
        elif isinstance(item, list):
            for child in item:
                visit(child)

    visit(value)
    return aliases


def main():
    reference = json.loads(
        gzip.decompress(
            (
                ROOT
                / "crates/nacre-helper/tests/fixtures/wallpaper-media-files-reference.json.gz"
            ).read_bytes()
        )
    )
    with tempfile.TemporaryDirectory(prefix="nacre-media-files-native-") as folder:
        home = Path(folder) / "home"
        home.mkdir()
        for name, entry in reference["inputs"].items():
            path = home / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(
                entry["text"].replace("/fixture/home", str(home)).encode()
                if "text" in entry
                else base64.b64decode(entry["base64"])
            )
            path.chmod(entry["mode"])
            os.utime(path, ns=(reference["input_mtime_ns"],) * 2)
        env = {
            **os.environ,
            "HOME": str(home),
            "XDG_CONFIG_HOME": str(home / ".config"),
            "XDG_STATE_HOME": str(home / ".local/state"),
            "XDG_CACHE_HOME": str(home / ".cache"),
            "NACRE_HELPER_TOOLS": historical_assets(home.parent),
            "XDG_DATA_DIRS": str(home.parent / "empty-system-data"),
            "NACRE_HELPER_BINARY": os.environ.get(
                "NACRE_HELPER_BINARY", str(ROOT / "target/debug/nacre-helper")
            ),
        }
        if "--python-reference" in sys.argv:
            raise ValueError("Use the archived original source for reference capture")
        worker = (
            "import json,sys\nfrom pathlib import Path\n"
            "sys.path.insert(0,sys.argv[1])\n"
            "from nacre_native_palette import wallpaper_media as media\n"
            + WORKER[WORKER.index("home=Path.home();") :]
        )
        result = subprocess.run(
            [
                sys.executable,
                "-c",
                worker,
                str(ROOT / "nacre/shell-tools"),
            ],
            env=env,
            capture_output=True,
            text=True,
            check=True,
            timeout=120,
        )
        results = json.loads(result.stdout)
        files = Files(home, [])
        files.discover(results)
        observed = files.normalize(results)
        observed_aliases = poster_aliases(observed)
        expected_aliases = poster_aliases(reference["results"])
        observed = normalize_posters(observed, observed_aliases)
        comparable_results = normalize_posters(reference["results"], expected_aliases)
        if observed != comparable_results:
            Path("/tmp/nacre-media-import-results-difference.json").write_text(
                json.dumps(
                    dict(actual=observed, expected=reference["results"]), indent=2
                )
            )
            raise AssertionError(
                "Import/catalog results differ; bounded artifact /tmp/nacre-media-import-results-difference.json"
            )
        actual = normalize_posters(files.snapshot(), observed_aliases)
        expected = normalize_posters(reference["outputs"], expected_aliases)
        assert set(actual) == set(expected), "Import/catalog file set differs"
        for name in expected:
            assert actual[name] == expected[name], (
                f"Import/catalog file differs: {name}"
            )
        print(
            f"Native imports/catalog: {len(results)} results and {len(actual)} files/links exact"
        )


if __name__ == "__main__":
    main()
