#!/usr/bin/env python3
"""Capture GIF canvas/transparency edge outputs without replacing prior references."""

import base64
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import struct
import sys
import tempfile
from PIL import Image

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "nacre/shell-tools/login-appearance.py"
DESTINATION = ROOT / "crates/nacre-helper/tests/fixtures/login-gif-edge-reference.json"


def capture():
    if DESTINATION.exists():
        raise ValueError("Refusing to overwrite GIF edge reference")
    source_sha = hashlib.sha256(SOURCE.read_bytes()).hexdigest()
    if source_sha != "20f04953a1324fa26ba640989b24aced6b4a8cd4914d687d0ec34375e066bd07":
        raise ValueError("Login source changed")
    image = Image.new("P", (17, 13))
    image.putpalette([200, 10, 50, 20, 220, 100] + [0] * 762)
    image.putdata([(x + y) % 2 for y in range(13) for x in range(17)])
    data = io.BytesIO()
    image.save(data, format="GIF", transparency=0, optimize=False)
    raw = data.getvalue()
    offset = bytearray(raw)
    offset[6:10] = struct.pack("<HH", 22, 20)
    descriptor = offset.index(0x2C, 13 + 3 * 256)
    offset[descriptor + 1 : descriptor + 5] = struct.pack("<HH", 4, 3)
    sys.path.insert(0, str(SOURCE.parent))
    spec = importlib.util.spec_from_file_location("gif_login_reference", SOURCE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    cases = []
    for name, encoded in [
        ("transparent-gif", raw),
        ("offset-transparent-gif", bytes(offset)),
    ]:
        with tempfile.TemporaryDirectory(prefix="nacre-login-gif-") as folder:
            source = Path(folder) / "input.gif"
            source.write_bytes(encoded)
            target, _ = module.prepare(source, Path(folder) / "cache")
            output = target.read_bytes()
            cases.append(
                {
                    "name": name,
                    "input": base64.b64encode(encoded).decode(),
                    "png": base64.b64encode(output).decode(),
                }
            )
    DESTINATION.write_text(
        json.dumps({"source_sha256": source_sha, "cases": cases}, indent=2) + "\n"
    )
    print("Frozen two GIF transparency/canvas cases")


if __name__ == "__main__":
    capture()
