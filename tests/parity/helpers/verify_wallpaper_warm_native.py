#!/usr/bin/env python3
"""Replay original warm/cache repair, preserving every physical cached file."""

import base64
import gzip
import hashlib
import re
import json
import os
from pathlib import Path
import subprocess
import tempfile

from colors_reference_files import Files
from freeze_wallpaper_warm import add_markers, file_list

from historical_palette_assets import stage as historical_assets

ROOT = Path(__file__).resolve().parents[3]


def main():
    reference = json.loads(
        gzip.decompress(
            (
                ROOT
                / "crates/nacre-helper/tests/fixtures/wallpaper-warm-reference.json.gz"
            ).read_bytes()
        )
    )
    with tempfile.TemporaryDirectory(prefix="nacre-warm-native-") as directory:
        base = Path(directory)
        home = base / "home"
        home.mkdir()
        binary = base / "bin"
        binary.mkdir()
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
        renderer = binary / "rsvg-convert"
        renderer.write_bytes(
            (ROOT / "tests/parity/helpers/svg_fixture.py").read_bytes()
        )
        renderer.chmod(0o700)
        responses = base / "svg.json"
        responses.write_text(json.dumps(reference["svg"]))
        env = {
            **os.environ,
            "HOME": str(home),
            "XDG_CONFIG_HOME": str(home / ".config"),
            "XDG_STATE_HOME": str(home / ".local/state"),
            "XDG_CACHE_HOME": str(home / ".cache"),
            "NACRE_HELPER_TOOLS": historical_assets(home.parent),
            "NACRE_LANGUAGE": "en",
            "NACRE_SVG_REFERENCE": str(responses),
            "PATH": str(binary) + ":/usr/bin",
        }
        steps = [
            dict(action="media-state", value=str(home / "scene.png")),
            dict(action="warm"),
            dict(action="warm"),
            dict(action="clear-branding-cache"),
            dict(action="warm"),
        ]
        result = subprocess.run(
            [str(ROOT / "target/debug/examples/wallpaper-media-parity")],
            input=json.dumps(dict(steps=steps)),
            env=env,
            capture_output=True,
            text=True,
            timeout=180,
        )
        if result.returncode:
            Path("/tmp/nacre-warm-native-worker.log").write_text(result.stderr)
            raise AssertionError(
                "Native warm failed; /tmp/nacre-warm-native-worker.log"
            )
        values = json.loads(result.stdout)
        values.pop(3)
        files = Files(home, [])
        files.discover(values)
        add_markers(home, files)
        assert files.normalize(values) == reference["results"], (
            "Warm/repair results differ"
        )
        observed = file_list(home, files)
        expected = reference["outputs"]

        def library_preview_names(rows):
            # The sole catalog image is an input; warm returns counts, not its descriptor.
            keys = {
                match[1]
                for row in rows
                if (
                    match := re.search(
                        r"/wallpaper-media/([0-9a-f]{64})-(?:thumbnail|preview)\.jpg$",
                        row["name"],
                    )
                )
            }
            assert len(keys) == 1, "Unexpected unlabelled warm preview identity"
            label = (
                "@MEDIA_"
                + hashlib.sha256(
                    b"/fixture/home/Pictures/Wallpapers/library.png"
                ).hexdigest()[:16]
                + "@"
            )
            old = next(iter(keys))
            result = []
            for row in rows:
                name = row["name"].replace(
                    "/wallpaper-media/" + old + "-", "/wallpaper-media/" + label + "-"
                )
                result.append({**row, "name": name})
            return sorted(result, key=lambda value: json.dumps(value, sort_keys=True))

        observed = library_preview_names(observed)
        expected = library_preview_names(expected)
        if observed != expected:
            Path("/tmp/nacre-warm-native-files-difference.json").write_text(
                json.dumps(dict(actual=observed, expected=expected), indent=2)
            )
            for index, (a, b) in enumerate(zip(observed, expected, strict=False)):
                if a != b:
                    raise AssertionError(
                        f"Warm file differs at index {index}: {a['name']} versus {b['name']}"
                    )
            raise AssertionError(
                f"Warm file count differs {len(observed)}/{len(expected)}"
            )
        print(
            f"Native warm: three preparation/cache-repair stages and all {len(observed)} files exact"
        )


if __name__ == "__main__":
    main()
