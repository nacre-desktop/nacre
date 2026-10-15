#!/usr/bin/env python3
"""Freeze the existing external chooser contract without opening a real dialog."""

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
DEST = ROOT / "crates/nacre-helper/tests/fixtures/wallpaper-pick-reference.json.gz"
MTIME = 1700000000123456789
PEER = """#!/usr/bin/python3
import json,os,sys
from pathlib import Path
Path(os.environ["PICK_LOG"]).write_text(json.dumps(sys.argv[1:]))
if os.environ["PICK_CASE"]=="cancel":sys.exit(1)
print(Path.home()/"outside/scene.png")
"""


def main():
    if DEST.exists():
        raise ValueError("Refusing to replace picker references")
    source_sha = hashlib.sha256(SOURCE.read_bytes()).hexdigest()
    if source_sha != "74795d256c7f4e44b5d19e6ed7b58a158d3a1905b552097c99de1681abcbc4a2":
        raise ValueError("Current media source changed")
    cases = []
    for name in ("cancel", "success"):
        with tempfile.TemporaryDirectory(prefix="nacre-pick-original-") as folder:
            base = Path(folder)
            home = base / "home"
            source = home / "outside/scene.png"
            source.parent.mkdir(parents=True)
            Image.new("RGB", (64, 40), "#172b45").save(source)
            os.utime(source, ns=(MTIME, MTIME))
            binary = base / "bin"
            binary.mkdir()
            peer = binary / "zenity"
            peer.write_text(PEER)
            peer.chmod(0o700)
            files = Files(home, [])
            inputs = files.snapshot()
            log = base / "calls.json"
            env = {
                **os.environ,
                "HOME": str(home),
                "XDG_CONFIG_HOME": str(home / ".config"),
                "XDG_STATE_HOME": str(home / ".local/state"),
                "XDG_CACHE_HOME": str(home / ".cache"),
                "NACRE_HELPER_BINARY": str(ROOT / "target/debug/nacre-helper"),
                "NACRE_HELPER_TOOLS": str(ROOT / "nacre/shell-tools"),
                "NACRE_LANGUAGE": "en",
                "PICK_CASE": name,
                "PICK_LOG": str(log),
                "PATH": str(binary) + ":/usr/bin",
            }
            result = subprocess.run(
                [sys.executable, str(SOURCE), "pick"],
                env=env,
                capture_output=True,
                text=True,
                timeout=30,
                check=True,
            )
            if name == "success":
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
            cases.append(
                dict(
                    name=name,
                    inputs=inputs,
                    stdout=files.normalize(result.stdout),
                    stderr=result.stderr,
                    exit=result.returncode,
                    effect=json.loads(log.read_text()),
                    outputs=files.snapshot(),
                )
            )
    DEST.write_bytes(
        gzip.compress(
            json.dumps(
                dict(source_sha256=source_sha, input_mtime_ns=MTIME, cases=cases),
                indent=2,
            ).encode(),
            mtime=0,
        )
    )
    print(f"Frozen {len(cases)} chooser/import states and exact dialog arguments")


if __name__ == "__main__":
    main()
