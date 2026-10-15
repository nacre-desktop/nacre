#!/usr/bin/env python3
"""Freeze Pillow geometry once, without overwriting reference pixels."""

import base64
import hashlib
import importlib.util
import json
import sys
from pathlib import Path
from PIL import Image, ImageDraw

root = Path(__file__).resolve().parents[3]
source = root / "nacre/shell-tools/lock-dashboard.py"
dest = root / "crates/nacre-helper/tests/fixtures/lock-geometry-reference.json"
assert not dest.exists()
assert (
    hashlib.sha256(source.read_bytes()).hexdigest()
    == "dfa7de6d0656a055edc3c5a288e20ff7699045938e48437394f531e43a6fccb1"
)
sys.path.insert(0, str(source.parent))
spec = importlib.util.spec_from_file_location("geometry_reference", source)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
cases = []
for size in (
    (1, 1),
    (2, 2),
    (11, 7),
    (16, 16),
    (37, 23),
    (38, 42),
    (60, 60),
    (126, 126),
):
    for radius in (0, 3, 21):
        im = module.rounded_mask(size, radius)
        cases.append(
            dict(
                operation="rounded_mask",
                size=size,
                radius=radius,
                pixels=base64.b64encode(im.tobytes()).decode(),
            )
        )
for size in (3, 12, 31, 126, 252, 540):
    im = module.avatar_mask(size)
    cases.append(
        dict(
            operation="avatar_mask",
            size=size,
            pixels=base64.b64encode(im.tobytes()).decode(),
        )
    )
for size in ((1, 1), (2, 2), (11, 7), (16, 16), (37, 23), (50, 50)):
    im = Image.new("L", size)
    ImageDraw.Draw(im).ellipse((0, 0, size[0] - 1, size[1] - 1), fill=255)
    cases.append(
        dict(
            operation="ellipse",
            size=size,
            pixels=base64.b64encode(im.tobytes()).decode(),
        )
    )
dest.write_text(
    json.dumps(
        dict(
            source_sha256=hashlib.sha256(source.read_bytes()).hexdigest(), cases=cases
        ),
        indent=2,
    )
    + "\n"
)
print("Frozen", len(cases), "geometry masks")
