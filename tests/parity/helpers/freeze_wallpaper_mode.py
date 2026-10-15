#!/usr/bin/env python3
"""Freeze Pillow automatic-mode thumbnail/luminance behavior before porting."""

import base64
import gzip
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile

from PIL import Image, ImageStat

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "nacre/shell-tools/wallpaper-media.py"
DEST = ROOT / "crates/nacre-helper/tests/fixtures/wallpaper-mode-reference.json.gz"


def main():
    if DEST.exists():
        raise ValueError("Refusing to overwrite automatic-mode references")
    if (
        hashlib.sha256(SOURCE.read_bytes()).hexdigest()
        != "74795d256c7f4e44b5d19e6ed7b58a158d3a1905b552097c99de1681abcbc4a2"
    ):
        raise ValueError("Media source changed")
    sys.path.insert(0, str(SOURCE.parent))
    spec = importlib.util.spec_from_file_location("original_media", SOURCE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    cases = []
    inputs = []
    for value in (0, 139, 140, 255):
        inputs.append((f"rgb-{value}", Image.new("RGB", (64, 64), (value,) * 3), "PNG"))
    inputs += [
        ("transparent-exact", Image.new("RGBA", (64, 64), (255, 255, 255, 0)), "PNG"),
        (
            "transparent-resized",
            Image.new("RGBA", (128, 64), (255, 255, 255, 0)),
            "PNG",
        ),
    ]
    for format_name in ("PNG", "GIF", "JPEG", "WEBP", "TIFF"):
        image = Image.new("RGB", (1536, 768), "#152a49")
        image.paste("#f5dfc2", (0, 0, 800, 768))
        if format_name in ("PNG", "GIF"):
            image = image.quantize(colors=16)
        inputs.append((format_name.lower() + "-resize", image, format_name))
    with tempfile.TemporaryDirectory(prefix="nacre-mode-original-") as directory:
        for name, image, format_name in inputs:
            raw = io.BytesIO()
            image.save(raw, format=format_name)
            path = Path(directory) / (name + ".input")
            path.write_bytes(raw.getvalue())
            with Image.open(path) as value:
                before = value.mode
                value.thumbnail((64, 64))
                luminance = value.convert("L")
                mean = ImageStat.Stat(luminance).mean[0]
                cases.append(
                    dict(
                        name=name,
                        input=base64.b64encode(raw.getvalue()).decode(),
                        original_mode=before,
                        thumbnail_mode=value.mode,
                        size=list(value.size),
                        luma=base64.b64encode(luminance.tobytes()).decode(),
                        mean=mean,
                        result=module.wallpaper_mode(path),
                    )
                )
    DEST.write_bytes(
        gzip.compress(
            json.dumps(
                dict(
                    source_sha256=hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
                    cases=cases,
                ),
                indent=2,
            ).encode(),
            mtime=0,
        )
    )
    print(
        f"Frozen {len(cases)} automatic-mode vectors, including palette/alpha/JPEG thumbnail paths"
    )


if __name__ == "__main__":
    main()
