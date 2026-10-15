#!/usr/bin/env python3
"""Freeze demo snapshots/entries and bounded history without applying a scene."""

import gzip
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

from PIL import Image
from colors_reference_files import Files

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "nacre/shell-tools/wallpaper-media.py"
DEST = ROOT / "crates/nacre-helper/tests/fixtures/wallpaper-demo-reference.json.gz"
TOKEN = "123456781234423482341234567890ab"
MTIME = 1700000000123456789
WORKER = """import importlib.util,json,sys,uuid
from pathlib import Path
source=Path(sys.argv[1]);sys.path.insert(0,str(source.parent))
spec=importlib.util.spec_from_file_location("original_media",source);media=importlib.util.module_from_spec(spec);spec.loader.exec_module(media)
uuid.uuid4=lambda:uuid.UUID(hex="123456781234423482341234567890ab")
result=media.demo_start();print(json.dumps(result))
"""


def main():
    if DEST.exists():
        raise ValueError("Refusing to replace demo references")
    digest = hashlib.sha256(SOURCE.read_bytes()).hexdigest()
    if digest != "74795d256c7f4e44b5d19e6ed7b58a158d3a1905b552097c99de1681abcbc4a2":
        raise ValueError("Current media source changed")
    cases = []
    for name in ("flat-start", "existing-start"):
        with tempfile.TemporaryDirectory(prefix="nacre-demo-original-") as directory:
            home = Path(directory) / "home"
            home.mkdir()
            scenes = home / "scenes"
            scenes.mkdir()
            for index, color in enumerate(("#112233", "#445566", "#778899", "#aabbcc")):
                Image.new("RGB", (32, 24), color).save(scenes / f"scene_{index}.png")
            config = home / ".config/nacre"
            config.mkdir(parents=True)
            (config / "welcome.json").write_text(
                json.dumps(
                    dict(
                        version=1,
                        demoWallpapers=[
                            str(scenes / f"scene_{i}.png") for i in range(4)
                        ],
                    )
                )
            )
            (config / "wallpaper-picker.json").write_text(
                '{"version":1,"paletteMode":"light","palettePreset":"super-red","rotationMinutes":90}'
            )
            state = home / ".local/state/nacre"
            history = state / "welcome/demo"
            history.mkdir(parents=True)
            for index in range(12):
                path = history / (f"{index:032x}.json")
                path.write_text('{"version":1,"public":true}')
                os.utime(path, ns=(MTIME + index,) * 2)
            if name == "existing-start":
                (state / "wallpaper").mkdir()
                scheme = json.loads(
                    (ROOT / "nacre/shell-tools/reference-style.json").read_text()
                )
                scheme["version"] = 1
                (state / "scheme.json").write_text(json.dumps(scheme))
                (state / "wallpaper/last.txt").write_text(str(scenes / "scene_0.png"))
            for path in scenes.iterdir():
                os.utime(path, ns=(MTIME, MTIME))
            files = Files(home, [])
            inputs = files.snapshot()
            env = {
                **os.environ,
                "HOME": str(home),
                "XDG_CONFIG_HOME": str(home / ".config"),
                "XDG_STATE_HOME": str(home / ".local/state"),
                "XDG_CACHE_HOME": str(home / ".cache"),
                "NACRE_HELPER_BINARY": str(ROOT / "target/debug/nacre-helper"),
                "NACRE_HELPER_TOOLS": str(ROOT / "nacre/shell-tools"),
                "NACRE_LANGUAGE": "en",
            }
            reply = subprocess.run(
                [sys.executable, "-c", WORKER, str(SOURCE)],
                env=env,
                capture_output=True,
                text=True,
                timeout=60,
                check=True,
            )
            result = json.loads(reply.stdout)
            snapshot = json.loads((history / (TOKEN + ".json")).read_text())
            files.discover([result, snapshot["media"]])
            cases.append(
                dict(
                    name=name,
                    inputs=inputs,
                    result=files.normalize(result),
                    outputs=files.snapshot(),
                )
            )
    DEST.write_bytes(
        gzip.compress(
            json.dumps(
                dict(
                    source_sha256=digest, token=TOKEN, input_mtime_ns=MTIME, cases=cases
                ),
                indent=2,
            ).encode(),
            mtime=0,
        )
    )
    print(f"Frozen {len(cases)} demo snapshot/entry/history cases; no scene published")


if __name__ == "__main__":
    main()
