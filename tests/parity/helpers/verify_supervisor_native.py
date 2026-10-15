#!/usr/bin/env python3
"""Compare native recovery against the original immutable Python outputs."""

import json
import os
from pathlib import Path
from supervisor_reference import capture, capture_api

ROOT = Path(__file__).resolve().parents[3]


def main():
    binary = Path(
        os.environ.get(
            "NACRE_SUPERVISOR_PARITY", ROOT / "target/debug/examples/supervisor-parity"
        )
    )
    for filename, api in (
        ("supervisor-reference.json", False),
        ("supervisor-api-reference.json", True),
        ("supervisor-signal-reference.json", False),
    ):
        original = json.loads(
            (ROOT / "crates/nacre-helper/tests/fixtures" / filename).read_text()
        )
        expected = original["api" if api else "cases"]
        actual = (
            capture_api([str(binary)])
            if api
            else capture([str(binary)], names=tuple(v["name"] for v in expected))
        )
        for before, after in zip(expected, actual, strict=True):
            if before["name"] == "terminate":
                # Only the fake tool's pre-exec trace is scheduler-dependent.
                # stdout, exit and every helper-written file remain exact; the
                # separate terminate-child reference checks actual forwarding.
                before = {k: v for k, v in before.items() if k != "launches"}
                after = {k: v for k, v in after.items() if k != "launches"}
            if before != after:
                artifact = (
                    Path(os.environ.get("TMPDIR", "/tmp"))
                    / "supervisor-parity-difference.json"
                )
                artifact.write_text(
                    json.dumps(dict(expected=before, actual=after), indent=2)
                )
                raise AssertionError(
                    f"Supervisor case {before['name']} differs: {artifact}"
                )
        print(filename, len(expected), "immutable contracts exact")


if __name__ == "__main__":
    main()
