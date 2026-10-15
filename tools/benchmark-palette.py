#!/usr/bin/env python3
"""Busy-machine helper CPU/launch/RSS comparison; never a desktop latency claim."""

import argparse
from collections import Counter
import importlib.util
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

from performance import snapshot

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "helper_microbenchmarks", ROOT / "tools/benchmark-helpers.py"
)
HELPERS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(HELPERS)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--baseline-engine", type=Path, required=True)
    parser.add_argument("--native", type=Path, required=True)
    parser.add_argument("--image", type=Path, required=True)
    parser.add_argument("--repeats", type=int, default=100)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error("--repeats must be positive")
    if not shutil.which("strace"):
        parser.error("strace is required to count actual process launches")
    baseline = args.baseline.resolve()
    native = args.native.resolve()
    pin = json.loads(
        (baseline / "nacre/shell-cli/src/nacre_shell/engine-pin.json").read_text()
    )
    info = subprocess.run(
        [str(args.baseline_engine.resolve()), "--rpc"],
        input=json.dumps({"operation": "info", "payload": {}}),
        capture_output=True,
        text=True,
        check=True,
    )
    if json.loads(info.stdout)["result"]["engine"] != pin["engine"]:
        raise ValueError("Baseline engine differs from its exact dependency pin")
    report = {
        "condition": "busy machine",
        "method": "benchmark-helpers.py child user+system CPU, interleaved fresh processes; performance.py helper-only RSS; strace execve counts",
        "baseline_commit": subprocess.check_output(
            ["git", "-C", str(baseline), "rev-parse", "HEAD"], text=True
        ).strip(),
        "baseline_engine": pin["engine"],
        "baseline_engine_sha256": hashlib.sha256(
            args.baseline_engine.read_bytes()
        ).hexdigest(),
        "native_sha256": hashlib.sha256(native.read_bytes()).hexdigest(),
        "native_engine": subprocess.check_output(
            [str(native), "nacre_shell", "--version"], text=True
        ).strip(),
        "input_sha256": hashlib.sha256(args.image.read_bytes()).hexdigest(),
        "results": {},
    }
    with tempfile.TemporaryDirectory(prefix="nacre-palette-benchmark-") as folder:
        base = Path(folder)
        jobs = {}
        palettes = []
        for side in ("python", "rust"):
            home = base / side
            home.mkdir()
            image = home / "input.png"
            image.write_bytes(args.image.read_bytes())
            env = dict(os.environ)
            for key in (
                "DISPLAY",
                "WAYLAND_DISPLAY",
                "NIRI_SOCKET",
                "HYPRLAND_INSTANCE_SIGNATURE",
                "XDG_CURRENT_DESKTOP",
                "NACRE_PALETTE_LOCKED",
            ):
                env.pop(key, None)
            env.update(
                HOME=str(home),
                XDG_CONFIG_HOME=str(home / ".config"),
                XDG_STATE_HOME=str(home / ".local/state"),
                XDG_CACHE_HOME=str(home / ".cache"),
                XDG_DATA_HOME=str(home / ".local/share"),
                XDG_RUNTIME_DIR=str(home / "runtime"),
                DBUS_SESSION_BUS_ADDRESS="unix:path=" + str(home / "absent-bus"),
                DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + str(home / "absent-system-bus"),
                NACRE_COMPOSITOR="hyprland",
                NACRE_LANGUAGE="en",
            )
            env["PYTHONPATH"] = os.pathsep.join(
                [
                    str(baseline / "nacre/shell-tools"),
                    str(baseline / "nacre/shell-cli/src"),
                ]
            )
            env["NACRE_ORIENT_BINARY"] = str(args.baseline_engine.resolve())
            env["NACRE_HELPER_TOOLS"] = str(ROOT / "nacre/shell-tools")
            command = (
                [sys.executable, str(baseline / "nacre/shell-tools/colors.py")]
                if side == "python"
                else [str(native), "colors"]
            )
            request = json.dumps({"image": str(image), "hour": 12}) + "\n"
            warm = subprocess.run(
                [*command, "preview"],
                input=request,
                env=env,
                capture_output=True,
                text=True,
                check=True,
                timeout=120,
            )
            palettes.append(json.loads(warm.stdout)["palette"]["colours"])
            jobs[side] = (command, env, request)
            report["results"][side] = {"calls": args.repeats, "cpu_seconds": 0.0}
        if palettes[0] != palettes[1]:
            raise ValueError("Before/after preview colors differ")
        for _ in range(args.repeats):
            for side, (command, env, request) in jobs.items():
                before = sum(HELPERS.cpu_times())
                subprocess.run(
                    [*command, "preview"],
                    input=request,
                    env=env,
                    capture_output=True,
                    text=True,
                    check=True,
                    timeout=120,
                )
                report["results"][side]["cpu_seconds"] += (
                    sum(HELPERS.cpu_times()) - before
                )
        for side, (command, env, request) in jobs.items():
            row = report["results"][side]
            row["cpu_ms_per_call"] = row.pop("cpu_seconds") * 1000 / args.repeats
            trace = base / (side + "-execve")
            subprocess.run(
                [
                    "strace",
                    "-f",
                    "-e",
                    "trace=execve",
                    "-o",
                    str(trace),
                    *command,
                    "preview",
                ],
                input=request,
                env=env,
                capture_output=True,
                text=True,
                check=True,
                timeout=120,
            )
            row["one_call_execs"] = dict(
                Counter(
                    Path(match[1]).name
                    for line in trace.read_text().splitlines()
                    if (match := re.search(r'execve\("([^"]+)".*= 0$', line))
                )
            )
            worker = subprocess.Popen(
                [*command, "serve"],
                env=env,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            try:
                if not json.loads(worker.stdout.readline()).get("ready"):
                    raise ValueError("Missing Colors ready packet")
                worker.stdin.write(
                    json.dumps({"sequence": 1, "request": json.loads(request)}) + "\n"
                )
                worker.stdin.flush()
                response = json.loads(worker.stdout.readline())
                if response.get("sequence") != 1 or response.get("error"):
                    raise ValueError("Unexpected Colors response")
                rows = snapshot(worker.pid, detached=False)
                if not rows or any(row.get("rss") is None for row in rows.values()):
                    raise ValueError("Helper memory unavailable")
                row["helper_rss_mib"] = (
                    sum(item["rss"] for item in rows.values()) / 1048576
                )
                row["helper_processes"] = len(rows)
            finally:
                worker.stdin.close()
                worker.wait(timeout=10)
                worker.stdout.close()
                worker.stderr.close()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
