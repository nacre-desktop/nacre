#!/usr/bin/env python3
"""Replay original chooser/import responses without opening any real window."""

import base64
import gzip
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

from colors_reference_files import Files
from freeze_wallpaper_pick import PEER

from historical_palette_assets import stage as historical_assets

ROOT = Path(__file__).resolve().parents[3]


def main():
    reference = json.loads(
        gzip.decompress(
            (
                ROOT
                / "crates/nacre-helper/tests/fixtures/wallpaper-pick-reference.json.gz"
            ).read_bytes()
        )
    )
    for case in reference["cases"]:
        with tempfile.TemporaryDirectory(prefix="nacre-pick-native-") as directory:
            base = Path(directory)
            home = base / "home"
            home.mkdir()
            binary = base / "bin"
            binary.mkdir()
            for name, entry in case["inputs"].items():
                path = home / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(
                    entry["text"].encode()
                    if "text" in entry
                    else base64.b64decode(entry["base64"])
                )
                path.chmod(entry["mode"])
                os.utime(path, ns=(reference["input_mtime_ns"],) * 2)
            peer = binary / "zenity"
            peer.write_text(PEER)
            peer.chmod(0o700)
            log = base / "calls.json"
            env = {
                **os.environ,
                "HOME": str(home),
                "XDG_CONFIG_HOME": str(home / ".config"),
                "XDG_STATE_HOME": str(home / ".local/state"),
                "XDG_CACHE_HOME": str(home / ".cache"),
                "NACRE_HELPER_TOOLS": historical_assets(home.parent),
                "NACRE_LANGUAGE": "en",
                "PICK_CASE": case["name"],
                "PICK_LOG": str(log),
                "PATH": str(binary) + ":/usr/bin",
            }
            reply = subprocess.run(
                [str(ROOT / "target/debug/nacre-helper"), "wallpaper-media", "pick"],
                env=env,
                capture_output=True,
                text=True,
                timeout=30,
            )
            files = Files(home, [])
            if case["name"] == "success":
                source = home / "outside/scene.png"
                key = hashlib.sha256(
                    (
                        str(source)
                        + str(source.stat().st_size)
                        + str(source.stat().st_mtime_ns)
                        + "preview-v1"
                    ).encode()
                ).hexdigest()
                files.aliases[key] = (
                    "@MEDIA_"
                    + hashlib.sha256(b"/fixture/home/outside/scene.png").hexdigest()[
                        :16
                    ]
                    + "@"
                )
            assert (files.normalize(reply.stdout), reply.stderr, reply.returncode) == (
                case["stdout"],
                case["stderr"],
                case["exit"],
            ), f"{case['name']}: chooser response differs"
            assert json.loads(log.read_text()) == case["effect"], (
                "Chooser arguments differ"
            )
            assert files.snapshot() == case["outputs"], (
                f"{case['name']}: chooser/import files differ"
            )
    print(f"Native picker: {len(reference['cases'])} chooser/import cases exact")


if __name__ == "__main__":
    main()
