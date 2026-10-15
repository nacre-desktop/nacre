#!/usr/bin/env python3
"""Freeze the current Python label/wrapping contract; refuse replacement."""

import gzip
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "nacre/shell-tools/lock-info.py"
DEST = ROOT / "crates/nacre-helper/tests/fixtures/lock-info-labels-reference.json.gz"
SOURCE_SHA = "aa38f4f85a6ba0035b23a35e7018c22df30a28eac1e0e043067e4529c74a2ad0"


def main():
    verify = "--verify-original" in sys.argv
    if DEST.exists() and not verify:
        raise ValueError("Refusing to replace lock label references")
    if hashlib.sha256(SOURCE.read_bytes()).hexdigest() != SOURCE_SHA:
        raise ValueError("Current lock-info reference source changed")
    sys.path.insert(0, str(SOURCE.parent))
    spec = importlib.util.spec_from_file_location("original_lock_info", SOURCE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    # fc-match is an input boundary: use an explicitly identified public font,
    # avoiding host-installed preference/fallback differences across CI machines.
    font = Path(
        subprocess.check_output(
            ["fc-match", "--format=%{file}", "DejaVu Sans:style=Bold"], text=True
        )
    )
    cases = []
    for value in (
        None,
        False,
        0,
        17,
        "",
        "A & B <tag> 'quoted'",
        "one\t two\nthree",
        "Words with spaces " * 24,
        "unbroken" * 80,
        "İstanbul · Ελληνικά · العربية · 😀",
        "x" * 2050,
    ):
        for width, lines, size in ((288, 2, 14), (80, 1, 16), (1, 2, 14)):
            cases.append(dict(operation="plain", args=[value, width, lines, size]))
    inputs = [
        {},
        dict(
            weather=dict(
                location="Oslo", temperature="14 °C", description="Cloudy & windy"
            )
        ),
        dict(weather=dict(error="Weather unavailable")),
        dict(media=dict(title="<Track>", artist="A & B", playing=True)),
        dict(media=dict(title="Long title " * 40, artist="Artist", playing=False)),
        dict(
            count=3,
            notifications=[
                dict(app="Mail", summary="Private title", body="Private body"),
                dict(app="Chat", summary="Hello", body="Body"),
                dict(app="Mail"),
            ],
        ),
    ]
    settings = [
        {},
        dict(lockWeather=False, lockMedia=False, lockNotifications=False),
        dict(lockNotificationContents=True),
    ]
    for data in inputs:
        for preferences in settings:
            for kind in (
                "weather",
                "media",
                "notifications",
                "status",
                "play-icon",
                "unknown",
            ):
                cases.append(dict(operation="label", args=[kind, data, preferences]))
    with tempfile.TemporaryDirectory(prefix="nacre-lock-label-reference-") as directory:
        root = Path(directory)
        power = root / "sys/class/power_supply"
        power.mkdir(parents=True)
        (power / "BAT0").mkdir()
        (power / "BAT0/capacity").write_text("63\n")
        original_path = module.Path

        def fixture_path(value):
            return (
                power
                if str(value) == "/sys/class/power_supply"
                else original_path(value)
            )

        with (
            patch.dict(
                os.environ,
                {
                    "HOME": str(root),
                    "XDG_CONFIG_HOME": str(root / ".config"),
                    "XDG_STATE_HOME": str(root / ".local/state"),
                    "XDG_CACHE_HOME": str(root / ".cache"),
                },
            ),
            patch.object(module, "HOME", root),
            patch.object(module, "CACHE", root / ".cache/nacre/lock-widgets.json"),
            patch.object(module.subprocess, "check_output", return_value=str(font)),
            patch.object(module, "Path", side_effect=fixture_path),
        ):
            for case in cases:
                try:
                    case["result"] = getattr(module, case["operation"])(*case["args"])
                except (ValueError, TypeError, IndexError) as error:
                    case["error"] = dict(type=type(error).__name__, message=str(error))
    result = dict(
        source_sha256=SOURCE_SHA,
        font=dict(
            family="DejaVu Sans",
            style="Bold",
            sha256=hashlib.sha256(font.read_bytes()).hexdigest(),
        ),
        battery_capacity="63",
        cases=cases,
    )
    if verify:
        if json.loads(gzip.decompress(DEST.read_bytes())) != result:
            raise AssertionError(
                "Original lock labels differ from their immutable capture"
            )
        print(
            f"Verified {len(cases)} original label cases in a disposable HOME; no output rewritten"
        )
        return
    DEST.write_bytes(
        gzip.compress(json.dumps(result, ensure_ascii=True, indent=2).encode(), mtime=0)
    )
    print(f"Frozen {len(cases)} original lock label/wrapping cases")


if __name__ == "__main__":
    main()
