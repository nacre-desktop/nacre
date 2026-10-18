#!/usr/bin/env python3
"""Additional immutable failures: terminal projection and display transaction rollback."""

import hashlib
import json
from pathlib import Path

from freeze_desktop_settings_main import SOURCE, capture

ROOT = Path(__file__).resolve().parents[3]
DEST = (
    ROOT
    / "crates/nacre-helper/tests/fixtures/desktop-settings-controller-rollback-reference.json"
)

if __name__ == "__main__":
    if DEST.exists():
        raise ValueError("Refusing to replace frozen settings rollback outputs")
    cases = []
    for backend in ("hyprland", "niri"):
        for failure, args in (
            ("mono-font-state", ["set", "monoFont", '"Fixture Mono"']),
            ("display-config", ["display", "extend-right", "DP-1"]),
        ):
            cases.append(dict(failure=failure, **capture(backend, args, failure)))
    DEST.write_text(
        json.dumps(
            dict(
                source_sha256=hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
                cases=cases,
            ),
            indent=2,
        )
        + "\n"
    )
    print("Frozen", len(cases), "terminal/display rollback contracts")
