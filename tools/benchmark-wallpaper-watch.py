#!/usr/bin/env python3
"""Measure isolated watcher CPU/RSS using the existing microbenchmark methods."""

import argparse
from collections import Counter
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import select
import shutil
import signal
import subprocess
import sys
import tempfile
import time

from performance import memory, snapshot

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "helper_microbenchmarks", ROOT / "tools/benchmark-helpers.py"
)
HELPERS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(HELPERS)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", required=True)
    parser.add_argument("--native", type=Path, required=True)
    parser.add_argument("--repeats", type=int, default=20)
    parser.add_argument("--idle-seconds", type=float, default=30)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.repeats < 1 or args.idle_seconds <= 0:
        parser.error("positive repetitions and idle duration required")
    if not shutil.which("strace"):
        parser.error("strace required for actual exec counts")
    original = subprocess.check_output(
        ["git", "show", args.baseline + ":nacre/shell-tools/wallpaper-watch.py"],
        cwd=ROOT,
    )
    report = dict(
        condition="busy machine",
        baseline_commit=subprocess.check_output(
            ["git", "rev-parse", args.baseline], cwd=ROOT, text=True
        ).strip(),
        source_sha256=hashlib.sha256(original).hexdigest(),
        native_sha256=hashlib.sha256(args.native.read_bytes()).hexdigest(),
        method="benchmark-helpers.py child CPU; performance.py helper-only RSS; strace execve; one small private library",
        clock_ticks_per_second=os.sysconf("SC_CLK_TCK"),
        results={},
    )
    with tempfile.TemporaryDirectory(prefix="nacre-watcher-benchmark-") as folder:
        base = Path(folder)
        source = base / "reference.py"
        source.write_bytes(original)
        jobs = {}
        for side in ("python", "rust"):
            home = base / side
            root = home / "Pictures/Wallpapers"
            root.mkdir(parents=True)
            engine = home / ".local/share/nacre/shell/bin/nacre_shell"
            engine.parent.mkdir(parents=True)
            (home / ".config/nacre").mkdir(parents=True)
            env = dict(
                os.environ,
                HOME=str(home),
                XDG_CONFIG_HOME=str(home / ".config"),
                XDG_STATE_HOME=str(home / ".local/state"),
                XDG_CACHE_HOME=str(home / ".cache"),
            )
            command = (
                [sys.executable, str(source)]
                if side == "python"
                else [str(args.native.resolve()), "wallpaper-watch"]
            )
            command += ["--root", str(root), "--engine", str(engine)]
            jobs[side] = (command, env, root)
            report["results"][side] = dict(
                calls=args.repeats, startup_event_cpu_seconds=0.0
            )

        def start(job, trace=None):
            command, env, root = job
            if trace:
                command = [
                    "strace",
                    "-f",
                    "-e",
                    "trace=execve",
                    "-o",
                    str(trace),
                    *command,
                ]
            child = subprocess.Popen(
                command,
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                start_new_session=True,
            )
            try:
                for attempt in range(10):
                    if child.poll() is not None:
                        raise AssertionError("Watcher exited before readiness")
                    (root / "ready.jpg").write_text(str(attempt))
                    if select.select([child.stdout], [], [], 1)[0]:
                        if json.loads(child.stdout.readline()) != {"changed": True}:
                            raise AssertionError("Invalid watcher packet")
                        return child
                raise AssertionError("No readiness event")
            except BaseException:
                stop(child)
                raise

        def stop(child):
            if child.poll() is None:
                os.killpg(child.pid, signal.SIGTERM)
            child.communicate(timeout=5)

        for _ in range(args.repeats):
            for side, job in jobs.items():
                before = sum(HELPERS.cpu_times())
                stop(start(job))
                report["results"][side]["startup_event_cpu_seconds"] += (
                    sum(HELPERS.cpu_times()) - before
                )
        for side, job in jobs.items():
            row = report["results"][side]
            row["startup_event_cpu_ms_per_call"] = (
                row.pop("startup_event_cpu_seconds") * 1000 / args.repeats
            )
            child = start(job)
            try:
                if len(snapshot(child.pid, detached=False)) != 1:
                    raise AssertionError("Watcher unexpectedly started descendants")
                first = HELPERS.cpu_times(child)
                samples = []
                started = time.monotonic()
                deadline = started + args.idle_seconds
                while time.monotonic() < deadline:
                    samples.append(memory(child.pid)["rss"] / 1048576)
                    time.sleep(min(0.5, max(0, deadline - time.monotonic())))
                last = HELPERS.cpu_times(child)
                row.update(
                    helper_rss_mib=sum(samples) / len(samples),
                    helper_processes=1,
                    idle_user_ms=(last[0] - first[0]) * 1000,
                    idle_sys_ms=(last[1] - first[1]) * 1000,
                    idle_seconds=time.monotonic() - started,
                )
            finally:
                stop(child)
            trace = base / (side + "-execve")
            stop(start(job, trace))
            row["one_call_execs"] = dict(
                Counter(
                    Path(match[1]).name
                    for line in trace.read_text().splitlines()
                    if (match := re.search(r'execve\("([^"]+)".*= 0$', line))
                )
            )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
