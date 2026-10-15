#!/usr/bin/env python3
"""Freeze the current palette CLI in fresh processes/private homes; never overwrite."""

import gzip
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
from PIL import Image

from colors_reference_files import Files
from freeze_palette_pipeline import ROOT, pooled

DEST = ROOT / "crates/nacre-helper/tests/fixtures/palette-cli-reference.json.gz"
SOURCE = ROOT / "nacre/shell-cli/src/nacre_shell/cli.py"


def capture():
    if DEST.exists():
        raise ValueError("Refusing to overwrite CLI reference")
    scenarios = [
        ("help", [["--help"]]),
        ("names", [["scheme", "list", "--names"]]),
        ("list", [["scheme", "list"]]),
        ("get", [["scheme", "get", "--json"]]),
        ("default", [["scheme", "set", "-n", "default", "-m", "light", "-f", "hard"]]),
        ("wallpaper", [["wallpaper", "-f", "{home}/scene.png", "--no-smart"]]),
        ("print", [["wallpaper", "-p", "{home}/scene.png"]]),
        (
            "changes",
            [
                ["wallpaper", "-f", "{home}/scene.png"],
                ["scheme", "set", "--accent", "2255aa"],
                ["scheme", "set", "-m", "light"],
                ["wallpaper", "--print"],
            ],
        ),
        ("missing", [["wallpaper", "-f", "{home}/missing.png"]]),
        ("invalid-mode", [["scheme", "set", "-m", "neon"]]),
        (
            "conflict",
            [["scheme", "set", "-n", "default", "--accent", "112233", "--auto-accent"]],
        ),
        ("jpeg-draft", [["wallpaper", "-f", "{home}/large.jpg", "--no-smart"]]),
    ]
    cases = []
    for name, commands in scenarios:
        with tempfile.TemporaryDirectory(prefix="nacre-cli-reference-") as directory:
            home = Path(directory)
            image = Image.new("RGB", (64, 40), "#172b45")
            image.paste("#db788f", (24, 12, 40, 28))
            image.save(home / "scene.png")
            if name == "jpeg-draft":
                image.resize((1536, 1536), Image.Resampling.NEAREST).save(
                    home / "large.jpg", quality=92
                )
            for path in home.iterdir():
                os.utime(path, ns=(1700000000123456789,) * 2)
            files = Files(home, [])
            before = files.snapshot()
            results = []
            environment = {
                **os.environ,
                "HOME": str(home),
                "XDG_CONFIG_HOME": str(home / ".config"),
                "XDG_STATE_HOME": str(home / ".local/state"),
                "XDG_CACHE_HOME": str(home / ".cache"),
                "PYTHONPATH": str(ROOT / "nacre/shell-cli/src"),
                "NACRE_LANGUAGE": "en",
            }
            environment.pop("NACRE_PALETTE_LOCKED", None)
            for command in commands:
                command = [word.replace("{home}", str(home)) for word in command]
                worker = subprocess.run(
                    [str(ROOT / ".venv/bin/python"), "-m", "nacre_shell", *command],
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
            files.discover(results)
            cases.append(
                dict(
                    name=name,
                    commands=commands,
                    inputs=before,
                    results=files.normalize(results),
                    outputs=files.snapshot(),
                )
            )
    value = dict(
        source_sha256=hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
        engine=json.loads((SOURCE.parent / "engine-pin.json").read_text())["engine"],
        normalizations="Disposable roots and derived Orient cache names only; input bytes/mtime, stdout/stderr/exit and all other file bytes/modes exact.",
        input_mtime_ns=1700000000123456789,
        cases=cases,
    )
    DEST.write_bytes(
        gzip.compress((json.dumps(pooled(value), indent=2) + "\n").encode(), mtime=0)
    )
    print(f"Frozen {len(cases)} CLI scenarios; {DEST.stat().st_size} compressed bytes")


if __name__ == "__main__":
    capture()
