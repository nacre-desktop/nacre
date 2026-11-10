#!/usr/bin/python3
"""Developer-only replay of immutable external SVG rasterizer responses."""

import base64
import hashlib
import json
import os
from pathlib import Path
import sys

records = json.loads(Path(os.environ["NACRE_SVG_REFERENCE"]).read_text())
source = sys.stdin.buffer.read()
for record in records:
    if (
        hashlib.sha256(record["svg"].encode()).digest()
        == hashlib.sha256(source).digest()
        and record["arguments"] == sys.argv[1:]
    ):
        sys.stdout.buffer.write(base64.b64decode(record["png"]))
        break
else:
    print("Unrecorded SVG rasterization request", file=sys.stderr)
    raise SystemExit(1)
