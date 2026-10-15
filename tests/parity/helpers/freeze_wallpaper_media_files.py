#!/usr/bin/env python3
"""Freeze imports/catalog/previews without modifying pre-existing reference files."""

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
DEST = (
    ROOT / "crates/nacre-helper/tests/fixtures/wallpaper-media-files-reference.json.gz"
)
MTIME = 1700000000123456789
WORKER = """import importlib.util,json,sys
from pathlib import Path
source=Path(sys.argv[1]);sys.path.insert(0,str(source.parent))
spec=importlib.util.spec_from_file_location("original_media",source)
media=importlib.util.module_from_spec(spec);spec.loader.exec_module(media)
home=Path.home();outside=home/"outside";results=[]
paths=[str(outside/"still.png"),(outside/"picture with space.png").as_uri(),str(outside/"motion.gif"),str(outside/"still.png")]
imported=media.import_files(paths);results.append(dict(imported=imported))
results.append(dict(already=media.import_files([imported[0]])))
for path in sorted(outside.iterdir()):results.append(media.describe(path))
results.append(media.catalog());results.append(media.catalog())
print(json.dumps(results))
"""


def main():
    if DEST.exists():
        raise ValueError("Refusing to replace media file references")
    source_sha = hashlib.sha256(SOURCE.read_bytes()).hexdigest()
    if source_sha != "74795d256c7f4e44b5d19e6ed7b58a158d3a1905b552097c99de1681abcbc4a2":
        raise ValueError("Current Python media reference changed")
    with tempfile.TemporaryDirectory(prefix="nacre-media-files-original-") as folder:
        home = Path(folder) / "home"
        outside = home / "outside"
        outside.mkdir(parents=True)
        Image.new("RGB", (64, 40), "#172b45").save(outside / "still.png")
        Image.new("RGB", (80, 40), "#db788f").save(outside / "picture with space.png")
        frames = [Image.new("RGB", (32, 24), color) for color in ("red", "blue")]
        frames[0].save(
            outside / "motion.gif",
            save_all=True,
            append_images=frames[1:],
            duration=100,
            loop=0,
        )
        library = home / "Pictures/Wallpapers"
        library.mkdir(parents=True)
        (library / "bad.jpg").write_bytes(b"invalid image")
        for path in home.rglob("*"):
            if path.is_file():
                os.utime(path, ns=(MTIME, MTIME))
        files = Files(home, [])
        inputs = files.snapshot()
        env = {
            **os.environ,
            "HOME": str(home),
            "XDG_CONFIG_HOME": str(home / ".config"),
            "XDG_STATE_HOME": str(home / ".local/state"),
            "XDG_CACHE_HOME": str(home / ".cache"),
            "NACRE_HELPER_BINARY": str(ROOT / "target/release/nacre-helper"),
            "NACRE_HELPER_TOOLS": str(ROOT / "nacre/shell-tools"),
            "NACRE_LANGUAGE": "en",
        }
        worker = subprocess.run(
            [sys.executable, "-c", WORKER, str(SOURCE)],
            env=env,
            capture_output=True,
            text=True,
            timeout=120,
            check=True,
        )
        results = json.loads(worker.stdout)
        files.discover(results)
        output = dict(
            source_sha256=source_sha,
            input_mtime_ns=MTIME,
            inputs=inputs,
            results=files.normalize(results),
            outputs=files.snapshot(),
        )
        DEST.write_bytes(gzip.compress(json.dumps(output, indent=2).encode(), mtime=0))
        print(
            f"Frozen import/catalog workflow: {len(results)} results and {len(output['outputs'])} files/links"
        )


if __name__ == "__main__":
    main()
