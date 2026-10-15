#!/usr/bin/env python3
"""Freeze current Python supervisor contracts once, before porting."""

import hashlib
import json
from pathlib import Path
import sys
from supervisor_reference import capture, capture_api

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "nacre/shell-tools/shell-supervisor.py"

if __name__ == "__main__":
    api = "--api" in sys.argv
    dest = (
        ROOT
        / "crates/nacre-helper/tests/fixtures"
        / ("supervisor-api-reference.json" if api else "supervisor-reference.json")
    )
    if dest.exists():
        raise ValueError("Refusing to overwrite frozen supervisor outputs")
    command = [
        sys.executable,
        str(ROOT / "tests/parity/helpers/supervisor_reference.py"),
    ]
    cases = capture_api(command) if api else capture(command)
    data = dict(
        source_sha256=hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
        clock="20261001T120000",
        **{"api" if api else "cases": cases},
    )
    dest.write_text(json.dumps(data, indent=2) + "\n")
    print("Frozen", len(cases), "supervisor contracts")
