#!/usr/bin/env python3
"""Freeze the current lock reader's command-line errors/help before cutover."""

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "nacre/shell-tools/lock-info.py"
DEST = ROOT / "crates/nacre-helper/tests/fixtures/lock-info-cli-reference.json"
SOURCE_SHA = "aa38f4f85a6ba0035b23a35e7018c22df30a28eac1e0e043067e4529c74a2ad0"


def main():
    if DEST.exists():
        raise ValueError("Refusing to replace lock CLI references")
    if hashlib.sha256(SOURCE.read_bytes()).hexdigest() != SOURCE_SHA:
        raise ValueError("Original lock-info source changed")
    cases = []
    for args in (
        [],
        ["--help"],
        ["unknown"],
        ["--wat"],
        ["weather", "--bad"],
        ["status", "extra"],
        ["--he"],
        ["--help=x"],
        ["-h", "--bad"],
        ["--", "--help"],
        ["art", "--help"],
    ):
        with tempfile.TemporaryDirectory(prefix="nacre-lock-cli-original-") as folder:
            home = Path(folder)
            result = subprocess.run(
                [sys.executable, str(SOURCE), *args],
                capture_output=True,
                text=True,
                env=dict(
                    os.environ,
                    HOME=str(home),
                    XDG_CONFIG_HOME=str(home / ".config"),
                    XDG_STATE_HOME=str(home / ".local/state"),
                    XDG_CACHE_HOME=str(home / ".cache"),
                    NACRE_LANGUAGE="en",
                ),
                timeout=10,
            )
            if list(home.rglob("*")):
                raise AssertionError("Lock help/errors unexpectedly wrote files")
            cases.append(
                dict(
                    args=args,
                    stdout=result.stdout,
                    stderr=result.stderr,
                    exit=result.returncode,
                )
            )
    DEST.write_text(
        json.dumps(dict(source_sha256=SOURCE_SHA, cases=cases), indent=2) + "\n"
    )
    print(f"Frozen {len(cases)} original lock CLI scenarios")


if __name__ == "__main__":
    main()
