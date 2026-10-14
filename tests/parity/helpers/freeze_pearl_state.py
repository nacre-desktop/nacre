#!/usr/bin/env python3
"""Freeze current panel state command output and every file written before porting."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "nacre/shell-tools/pearl-state.py"
SOURCE_SHA = "778a05bc8727c3cc34097c8cc25f8f5082aae7f33c7e0f60e93bc1f903c8a148"
DESTINATION = ROOT / "crates/nacre-helper/tests/fixtures/pearl-state-reference.json"


def verify_source():
    if (
        not SOURCE.is_file()
        or hashlib.sha256(SOURCE.read_bytes()).hexdigest() != SOURCE_SHA
    ):
        raise ValueError("Pearl state reference source is absent or changed")


def requests():
    for name, command, initial, input_text in (
        ("missing", "load", None, ""),
        ("empty-layout", "save", None, '{"version":1,"windows":[]}\n'),
        (
            "layout-unicode",
            "save",
            None,
            json.dumps(
                {
                    "version": 1,
                    "windows": [
                        {"kind": "launcher", "initial": {"query": "fixture ç"}}
                    ],
                    "ai": {"edge": "panel", "channels": ["", "panel"]},
                }
            )
            + "\n",
        ),
        ("existing", "load", '{"version":1,"windows":[],"future":"retained"}', ""),
        ("corrupt", "load", "bad saved data", ""),
        ("wrong-version", "load", '{"version":2,"windows":[]}', ""),
        ("non-object", "load", "[]", ""),
        (
            "invalid-save",
            "save",
            '{"version":1,"windows":[]}',
            '{"version":1,"windows":{}}\n',
        ),
        ("save-bad-json", "save", '{"version":1,"windows":[]}', "invalid\n"),
        ("one-line-only", "save", None, '{"version":1,"windows":[]}\ninvalid\n'),
        ("boolean-version", "save", None, '{"version":true,"windows":[]}\n'),
    ):
        yield {
            "name": name,
            "command": command,
            "initial": initial,
            "input": input_text,
        }


def capture(destination):
    verify_source()
    if destination.exists():
        raise ValueError("Refusing to overwrite frozen pearl state outputs")
    cases = []
    for request in requests():
        with tempfile.TemporaryDirectory(prefix="pearl-state-reference-") as folder:
            home = Path(folder)
            path = home / ".local/state/nacre/shell/pearls.json"
            if request["initial"] is not None:
                path.parent.mkdir(parents=True)
                path.write_text(request["initial"])
            env = {
                "HOME": folder,
                "PATH": "",
                "PYTHONDONTWRITEBYTECODE": "1",
                "NACRE_LANG": "en",
                "XDG_CONFIG_HOME": folder + "/.config",
                "XDG_STATE_HOME": folder + "/.local/state",
                "XDG_CACHE_HOME": folder + "/.cache",
            }
            result = subprocess.run(
                [sys.executable, str(SOURCE), request["command"]],
                input=request["input"],
                env=env,
                capture_output=True,
                text=True,
                timeout=5,
            )
            files = {
                str(file.relative_to(home)): {
                    "hex": file.read_bytes().hex(),
                    "mode": oct(file.stat().st_mode & 0o777),
                }
                for file in sorted(home.rglob("*"))
                if file.is_file()
            }
            cases.append(
                {
                    "request": request,
                    "stdout": result.stdout,
                    "stderr": result.stderr,
                    "exit": result.returncode,
                    "files": files,
                    "parent_mode": oct(path.parent.stat().st_mode & 0o777)
                    if path.parent.exists()
                    else None,
                }
            )
    verify_source()
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("x") as output:
        output.write(
            json.dumps({"source_sha256": SOURCE_SHA, "cases": cases}, indent=2) + "\n"
        )
    return len(cases)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=DESTINATION)
    args = parser.parse_args()
    print("Frozen pearl state cases:", capture(args.output))


if __name__ == "__main__":
    main()
