#!/usr/bin/env python3
"""Add immutable artwork cases from the unchanged Python module, once."""

import base64
import hashlib
import io
import json
from pathlib import Path
import subprocess
import sys

from PIL import Image

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "nacre/shell-tools/lock-info.py"
SOURCE_SHA = "aa38f4f85a6ba0035b23a35e7018c22df30a28eac1e0e043067e4529c74a2ad0"
DEST = ROOT / "crates/nacre-helper/tests/fixtures/lock-artwork-provenance.json"


def main():
    if DEST.exists():
        raise ValueError("Refusing to replace original lock artwork provenance")
    if hashlib.sha256(SOURCE.read_bytes()).hexdigest() != SOURCE_SHA:
        raise ValueError("Original lock artwork source changed")
    folder = ROOT / "tests/parity/helpers/lock-info"
    images = []
    for name, mode, size, format_name in (
        ("rgb", "RGB", (64, 40), "PNG"),
        ("rgba-resize", "RGBA", (1024, 640), "PNG"),
        ("indexed-resize", "P", (768, 384), "PNG"),
        ("gif-resize", "P", (768, 384), "GIF"),
        ("jpeg-resize", "RGB", (1536, 768), "JPEG"),
        ("webp-alpha", "RGBA", (64, 40), "WEBP"),
    ):
        image = Image.new("RGBA" if mode == "RGBA" else "RGB", size, "#172b45")
        image.paste("#e8af77", (0, 0, size[0] // 2, size[1]))
        if mode == "RGBA":
            image.putalpha(100)
        if mode == "P":
            image = image.quantize(colors=8)
        output = io.BytesIO()
        image.save(output, format_name)
        images.append((name, output.getvalue()))
    images += [("malformed", b"not an image"), ("bounded", b"x" * 4000002)]
    names = ["native-capture-art-" + name for name, _ in images]
    if any(
        (folder / f"{prefix}-{name}.json").exists()
        for prefix in ("case", "expected")
        for name in names
    ):
        raise ValueError("Refusing to replace any existing artwork case")
    for name, (_, raw) in zip(names, images, strict=True):
        case = dict(
            helper="lock-info",
            command="python3 tests/parity/helpers/api.py nacre/shell-tools/lock-info.py",
            args=[
                "artwork",
                json.dumps(
                    dict(
                        args=[dict(media=dict(art="file://{home}/art.png"))], kwargs={}
                    )
                ),
            ],
            fixtures={
                "home/art.png": dict(
                    base64=base64.b64encode(raw).decode(), mtime=1700000000
                ),
                "home/.cache/nacre": dict(directory=True),
            },
            normalizations=[
                dict(
                    kind="timestamp",
                    pointer="/files/home~1.cache~1nacre~1lock-art-source.json/text",
                    json_pointer="/checked",
                    reason="art freshness check time.time()",
                )
            ],
        )
        (folder / f"case-{name}.json").write_text(json.dumps(case, indent=2) + "\n")
    subprocess.run(
        [
            sys.executable,
            str(ROOT / "tests/parity/helpers/run.py"),
            "--record",
            "--case",
            "lock-info/native-capture-art-*",
        ],
        check=True,
    )
    records = {
        name: hashlib.sha256(
            (folder / f"expected-{name}.json").read_bytes()
        ).hexdigest()
        for name in names
    }
    DEST.write_text(
        json.dumps(dict(source_sha256=SOURCE_SHA, expected_sha256=records), indent=2)
        + "\n"
    )
    print(f"Frozen {len(names)} original artwork cases in the isolated corpus harness")


if __name__ == "__main__":
    main()
