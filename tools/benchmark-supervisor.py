#!/usr/bin/env python3
"""Busy-machine supervisor CPU and resident helper-group RSS; private fake QS."""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import tempfile
import time

from performance import snapshot

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "helper_bench", ROOT / "tools/benchmark-helpers.py"
)
helpers = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(helpers)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--python-source", type=Path, required=True)
    parser.add_argument("--native", type=Path, required=True)
    parser.add_argument("--repeats", type=int, default=20)
    parser.add_argument("--idle-seconds", type=float, default=30)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.repeats < 1 or args.idle_seconds <= 0:
        parser.error("Positive repetitions and idle duration required")
    results = {side: dict(cpu_ms=0, python_launches=0) for side in ("python", "native")}
    with tempfile.TemporaryDirectory(prefix="nacre-supervisor-bench-") as folder:
        base = Path(folder)

        def start(side, name):
            home = base / side / str(name)
            runtime = home / "runtime"
            runtime.mkdir(parents=True, mode=0o700)
            qs = home / ".local/share/nacre/shell/bin/qs"
            qs.parent.mkdir(parents=True)
            qs.write_text("""#!/bin/sh
case "$*" in *"ipc call"*)
 test -f "$HOME/running" || exit 1
 printf '{"reveal":0,"launcherMode":"apps"}\\n'
 touch "$HOME/ready"
 exit 0;; esac
trap 'kill "$sleeper" 2>/dev/null; wait "$sleeper" 2>/dev/null; exit 143' TERM INT
sleep 300 & sleeper=$!
touch "$HOME/running"
wait "$sleeper"
""")
            qs.chmod(0o700)
            peer = socket.socket(socket.AF_UNIX)
            peer.bind(str(runtime / "wayland-test"))
            env = dict(
                PATH="/usr/bin:/bin",
                HOME=str(home),
                XDG_CONFIG_HOME=str(home / ".config"),
                XDG_DATA_HOME=str(home / ".local/share"),
                XDG_STATE_HOME=str(home / ".local/state"),
                XDG_CACHE_HOME=str(home / ".cache"),
                XDG_RUNTIME_DIR=str(runtime),
                WAYLAND_DISPLAY="wayland-test",
                NACRE_COMPOSITOR="niri",
                NIRI_SOCKET=str(runtime / "absent-niri"),
                NACRE_HELPER_BINARY=str(args.native.resolve()),
                PYTHONPATH=str(ROOT / "nacre/shell-tools"),
                DBUS_SESSION_BUS_ADDRESS="unix:path=" + str(runtime / "absent-session"),
                DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + str(runtime / "absent-system"),
            )
            command = (
                [sys.executable, str(args.python_source)]
                if side == "python"
                else [str(args.native.resolve()), "shell-supervisor"]
            )
            child = subprocess.Popen(
                command,
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                start_new_session=True,
            )
            try:
                deadline = time.monotonic() + 5
                while not (home / "ready").is_file():
                    if child.poll() is not None or time.monotonic() > deadline:
                        raise AssertionError(
                            "Private supervisor did not reach QS readiness"
                        )
                    time.sleep(0.01)
                return child, peer
            except BaseException:
                try:
                    os.killpg(child.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                child.wait()
                peer.close()
                raise

        def stop(child, peer):
            try:
                child.send_signal(signal.SIGTERM)
                stdout, stderr = child.communicate(timeout=5)
                if child.returncode:
                    raise AssertionError(
                        f"Supervisor termination failed: {stdout[-200:]!r} {stderr[-400:]!r}"
                    )
            finally:
                try:
                    os.killpg(child.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                child.wait()
                peer.close()

        for number in range(args.repeats):
            for side in ("python", "native"):
                before = sum(helpers.cpu_times())
                stop(*start(side, number))
                results[side]["cpu_ms"] += (
                    (sum(helpers.cpu_times()) - before) * 1000 / args.repeats
                )
                results[side]["python_launches"] += int(side == "python")
        for side in ("python", "native"):
            child, peer = start(side, "resident")
            try:
                samples = []
                start_cpu = None
                end_cpu = None
                deadline = time.monotonic() + args.idle_seconds
                counts = set()
                while time.monotonic() < deadline:
                    rows = snapshot(child.pid, detached=False)
                    selected = [
                        row
                        for row in rows.values()
                        if row["pid"] == child.pid or b"serve-api" in row["args"]
                    ]
                    if not selected or any(row["swap_pss"] != 0 for row in selected):
                        raise AssertionError("Missing or swapped measured supervisor")
                    counts.add(len(selected))
                    samples.append(sum(row["rss"] for row in selected) / 1048576)
                    end_cpu = sum(row["ticks"] for row in selected) / os.sysconf(
                        "SC_CLK_TCK"
                    )
                    if start_cpu is None:
                        start_cpu = end_cpu
                    time.sleep(0.5)
                results[side].update(
                    helper_group_rss_mib=sum(samples) / len(samples),
                    helper_process_counts=sorted(counts),
                    idle_cpu_seconds=end_cpu - start_cpu,
                    idle_seconds=args.idle_seconds,
                )
            finally:
                stop(child, peer)
    report = dict(
        source_sha256=hashlib.sha256(args.python_source.read_bytes()).hexdigest(),
        native_sha256=hashlib.sha256(args.native.read_bytes()).hexdigest(),
        clock_ticks_per_second=os.sysconf("SC_CLK_TCK"),
        condition="busy machine",
        method="benchmark-helpers.py child CPU includes identical fake QS work; performance.py helper-group RSS excludes fake QS/sleep",
        results=results,
    )
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
