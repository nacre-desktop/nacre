#!/usr/bin/env python3
"""Compare cached lock-reader CPU with the established helper benchmark method."""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "helper_cpu", ROOT / "tools/benchmark-helpers.py"
)
HELPERS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(HELPERS)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline-tools", type=Path, required=True)
    parser.add_argument("--native", type=Path, required=True)
    parser.add_argument("--repeats", type=int, default=100)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error("positive repetition count required")
    source = args.baseline_tools.resolve() / "lock-info.py"
    native = args.native.resolve()
    report = dict(
        condition="busy machine",
        source_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),
        native_sha256=hashlib.sha256(native.read_bytes()).hexdigest(),
        method="benchmark-helpers.py child user+system CPU; fresh process; cached snapshot; no shell wall-time claim",
        results={},
    )
    with tempfile.TemporaryDirectory(prefix="nacre-lock-bench-") as folder:
        home = Path(folder)
        cache = home / ".cache/nacre/lock-widgets.json"
        cache.parent.mkdir(parents=True)
        (home / "runtime").mkdir(mode=0o700)
        env = dict(
            os.environ,
            HOME=str(home),
            XDG_CONFIG_HOME=str(home / ".config"),
            XDG_DATA_HOME=str(home / ".local/share"),
            XDG_STATE_HOME=str(home / ".local/state"),
            XDG_CACHE_HOME=str(home / ".cache"),
            XDG_RUNTIME_DIR=str(home / "runtime"),
            DBUS_SESSION_BUS_ADDRESS="unix:path=" + str(home / "absent-bus"),
            DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + str(home / "absent-system"),
            NIRI_SOCKET=str(home / "absent-niri"),
            HYPRLAND_INSTANCE_SIGNATURE="absent-fixture",
            NACRE_COMPOSITOR="hyprland",
            NACRE_LANGUAGE="en",
            PYTHONPATH=str(source.parent),
            NACRE_HELPER_TOOLS=str(ROOT / "nacre/shell-tools"),
        )
        for name in (
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "FONTCONFIG_FILE",
            "FONTCONFIG_PATH",
            "FONTCONFIG_SYSROOT",
        ):
            env.pop(name, None)
        commands = {
            "python": [sys.executable, str(source), "play-icon"],
            "rust": [str(native), "lock-info", "play-icon"],
        }
        for side in commands:
            report["results"][side] = dict(calls=args.repeats, cpu_seconds=0.0)
        for _ in range(args.repeats):
            for side, command in commands.items():
                cache.write_text(
                    json.dumps(
                        dict(time=time.time(), data=dict(media=dict(playing=True)))
                    )
                )
                before = sum(HELPERS.cpu_times())
                result = subprocess.run(
                    command,
                    env=env,
                    text=True,
                    capture_output=True,
                    check=True,
                    timeout=10,
                )
                report["results"][side]["cpu_seconds"] += (
                    sum(HELPERS.cpu_times()) - before
                )
                if result.stdout != "pause\n":
                    raise AssertionError("Cached lock-reader result differs")
        for side, row in report["results"].items():
            row["cpu_ms_per_call"] = 1000 * row.pop("cpu_seconds") / args.repeats
            row["python_launches"] = args.repeats if side == "python" else 0
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
