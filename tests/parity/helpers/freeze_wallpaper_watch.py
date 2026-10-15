#!/usr/bin/env python3
"""Freeze current watcher behavior once, before the native cutover."""

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

from wallpaper_watch_reference import capture

ROOT = Path(__file__).resolve().parents[3]
DEST = ROOT / "crates/nacre-helper/tests/fixtures/wallpaper-watch-reference.json"
SOURCE = ROOT / "nacre/shell-tools/wallpaper-watch.py"


def main():
    flavour_dest = DEST.with_name("wallpaper-watch-flavour-reference.json")
    options_dest = DEST.with_name("wallpaper-watch-cli-options-reference.json")
    if any(path.exists() for path in (DEST, flavour_dest, options_dest)):
        raise ValueError("Refusing to replace watcher references")
    if (
        hashlib.sha256(SOURCE.read_bytes()).hexdigest()
        != "8ef51a4fa93e9252dfe74816d3f3ea737f8b8d4d932aecdc78f1be1043fc4f6c"
    ):
        raise ValueError("Watcher source differs from the current reference")
    cli = []
    for args in (["--help"], ["--root"], ["--unknown", "value"]):
        with tempfile.TemporaryDirectory(prefix="nacre-watch-cli-") as home:
            result = subprocess.run(
                [sys.executable, str(SOURCE), *args],
                env={**os.environ, "HOME": home},
                capture_output=True,
                text=True,
            )
            cli.append(
                dict(
                    args=args,
                    stdout=result.stdout,
                    stderr=result.stderr,
                    exit=result.returncode,
                )
            )
    module_cases = []
    with tempfile.TemporaryDirectory(prefix="nacre-watch-flavour-reference-") as folder:
        path = Path(folder) / "scheme.json"
        for name, text in (
            ("default", "{}"),
            ("all-fields", '{"flavour":"hard","mode":"light","variant":"vibrant"}'),
            ("typed-fields", '{"flavour":null,"mode":3,"variant":true}'),
            ("malformed", "broken JSON"),
            ("missing", None),
            ("array", "[]"),
        ):
            if text is None:
                path.unlink(missing_ok=True)
            else:
                path.write_text(text)
            result = subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "tests/parity/helpers/api.py"),
                    str(SOURCE),
                    "flavour",
                    json.dumps(dict(args=[{"$path": str(path)}], kwargs={})),
                ],
                env={**os.environ, "HOME": folder},
                capture_output=True,
                text=True,
                check=False,
            )
            module_cases.append(
                dict(
                    name=name, input=text, stdout=result.stdout, exit=result.returncode
                )
            )
    options = []
    for args in (
        ["--h"],
        ["--r", "folder", "--h"],
        ["--help=x"],
        ["--root", "--scheme", "x"],
        ["--root", "-1", "--help"],
        ["--scheme", "-.5", "--help"],
        ["--root", "-1e2", "--help"],
        ["--engine=-x", "--help"],
        ["--bad", "ignored", "--help"],
        ["--", "--help"],
    ):
        with tempfile.TemporaryDirectory(
            prefix="nacre-watch-options-original-"
        ) as folder:
            result = subprocess.run(
                [sys.executable, str(SOURCE), *args],
                env={**os.environ, "HOME": folder},
                capture_output=True,
                text=True,
                check=False,
            )
            options.append(
                dict(
                    args=args,
                    stdout=result.stdout,
                    stderr=result.stderr,
                    exit=result.returncode,
                )
            )
    for path, cases in ((flavour_dest, module_cases), (options_dest, options)):
        path.write_text(
            json.dumps(
                dict(
                    source_sha256=hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
                    cases=cases,
                ),
                indent=2,
            )
            + "\n"
        )
    dialogue = capture([sys.executable, str(SOURCE)])
    DEST.write_text(
        json.dumps(
            dict(
                source_sha256=hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
                cli=cli,
                dialogue=dialogue,
            ),
            indent=2,
        )
        + "\n"
    )
    print(
        f"Frozen {len(cli)} CLI cases and {len(dialogue['steps'])} event steps, exit {dialogue['exit']}"
    )


if __name__ == "__main__":
    main()
