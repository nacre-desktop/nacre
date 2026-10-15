#!/usr/bin/env python3
"""Private busy-machine session check CPU and native observer RSS/idle ticks."""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import select
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time

from performance import snapshot

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tests/parity/helpers"))
from native_compositor_peer import Peer  # noqa: E402 - developer fixture path above

SPEC = importlib.util.spec_from_file_location(
    "helper_bench", ROOT / "tools/benchmark-helpers.py"
)
helpers = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(helpers)


def stop(child):
    if child is None:
        return
    try:
        os.killpg(child.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        child.wait(timeout=5)
    except subprocess.TimeoutExpired:
        os.killpg(child.pid, signal.SIGKILL)
        child.wait(timeout=5)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--python-source", type=Path, required=True)
    parser.add_argument("--native", type=Path, required=True)
    parser.add_argument("--bus-provider", type=Path, required=True)
    parser.add_argument("--repeats", type=int, default=20)
    parser.add_argument("--idle-seconds", type=float, default=30)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.repeats < 1 or args.idle_seconds <= 0:
        parser.error("Positive repetitions and idle duration required")
    report = dict(machine="busy machine", repeats=args.repeats, python={}, native={})
    with tempfile.TemporaryDirectory(prefix="session-bench-") as folder:
        root = Path(folder)
        rules = [
            dict(argv=[name, "-j"], stdout="{}" if name == "activewindow" else "[]")
            for name in ("monitors", "workspaces", "clients", "activewindow")
        ]
        rules.append(dict(argv=["configerrors"], stdout=""))
        peer = Peer(root, rules)
        events = socket.socket(socket.AF_UNIX)
        events.bind(str(root / "runtime/hypr/fixture/.socket2.sock"))
        events.listen()
        events.settimeout(0.1)
        streams = []
        done = threading.Event()

        def accept():
            while not done.is_set():
                try:
                    streams.append(events.accept()[0])
                except TimeoutError:
                    continue
                except OSError:
                    return

        listener = threading.Thread(target=accept)
        listener.start()
        daemon = provider = observer = None
        try:
            address = "unix:path=" + str(root / "bus")
            daemon = subprocess.Popen(
                [
                    "dbus-daemon",
                    "--session",
                    "--nofork",
                    "--print-address",
                    "--address=" + address,
                ],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                start_new_session=True,
            )
            if not select.select([daemon.stdout], [], [], 5)[
                0
            ] or not daemon.stdout.readline().startswith("unix:"):
                raise RuntimeError("Private bus did not start")
            provider = subprocess.Popen(
                [str(args.bus_provider.resolve()), address],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                start_new_session=True,
            )
            if (
                not select.select([provider.stdout], [], [], 5)[0]
                or provider.stdout.readline().strip() != "ready"
            ):
                raise RuntimeError("Private wallet/login fixture did not start")

            def environment(name):
                home = root / name
                qs = home / ".local/share/nacre/shell/bin/qs"
                qs.parent.mkdir(parents=True)
                qs.write_text('#!/bin/sh\nprintf \'{"launcherMode":"apps"}\\n\'\n')
                qs.chmod(0o700)
                return dict(
                    PATH="/usr/bin:/bin",
                    HOME=str(home),
                    XDG_CONFIG_HOME=str(home / ".config"),
                    XDG_STATE_HOME=str(home / ".local/state"),
                    XDG_CACHE_HOME=str(home / ".cache"),
                    XDG_DATA_HOME=str(home / ".local/share"),
                    XDG_RUNTIME_DIR=str(root / "runtime"),
                    NACRE_COMPOSITOR="hyprland",
                    HYPRLAND_INSTANCE_SIGNATURE="fixture",
                    NACRE_HELPER_BINARY=str(args.native.resolve()),
                    PYTHONPATH=str(ROOT / "nacre/shell-tools"),
                    DBUS_SESSION_BUS_ADDRESS=address,
                    DBUS_SYSTEM_BUS_ADDRESS=address,
                )

            def command(side, action):
                return (
                    [sys.executable, str(args.python_source.resolve())]
                    if side == "python"
                    else [str(args.native.resolve()), "session-watch"]
                ) + [action]

            for side in ("python", "native"):
                report[side]["cpu_ms"] = 0
                report[side]["python_launches"] = (
                    args.repeats if side == "python" else 0
                )
            for repeat in range(args.repeats):
                for side in ("python", "native"):
                    env = environment(f"{side}-{repeat}")
                    before = helpers.cpu_times()
                    output = subprocess.run(
                        command(side, "check"),
                        env=env,
                        text=True,
                        capture_output=True,
                        timeout=20,
                        check=True,
                    )
                    after = helpers.cpu_times()
                    record = json.loads(output.stdout)
                    if not record["desktopReady"] or record["wallet"] != "unlocked":
                        raise AssertionError("Check fixture did not reach ready state")
                    report[side]["cpu_ms"] += (
                        (sum(after) - sum(before)) * 1000 / args.repeats
                    )
            # Record the current Python observer's actual failure, not a fabricated idle comparison.
            env = environment("python-watch")
            observer = subprocess.Popen(
                command("python", "watch"),
                env=env,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                start_new_session=True,
            )
            _, error = observer.communicate(timeout=20)
            if (
                observer.returncode == 0
                or "Unknown compositor operation: event_socket" not in error
            ):
                raise AssertionError(
                    "Python observer outcome differs from the known bridge failure: "
                    + error[-500:]
                )
            report["python"].update(
                observer_exit=observer.returncode,
                idle_rss_mib=None,
                idle_ticks=None,
                reason="Retired event_socket bridge operation; no valid resident baseline",
            )
            stop(observer)
            env = environment("native-watch")
            observer = subprocess.Popen(
                command("native", "watch"),
                env=env,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                start_new_session=True,
            )
            log = Path(env["XDG_STATE_HOME"]) / "nacre/session/events.jsonl"
            deadline = time.monotonic() + 10
            while not log.is_file() or not streams:
                if observer.poll() is not None or time.monotonic() > deadline:
                    raise AssertionError("Native observer failed to subscribe")
                time.sleep(0.01)
            time.sleep(0.1)
            before = snapshot(observer.pid, detached=False)
            time.sleep(args.idle_seconds)
            after = snapshot(observer.pid, detached=False)
            if set(before) != set(after) or observer.poll() is not None:
                raise AssertionError("Native resident process set changed")
            report["native"].update(
                idle_rss_mib=sum(row["rss"] for row in after.values()) / 1048576,
                idle_ticks=sum(
                    row["ticks"]
                    + row["child_ticks"]
                    - before[pid]["ticks"]
                    - before[pid]["child_ticks"]
                    for pid, row in after.items()
                ),
                processes=len(after),
            )
        finally:
            stop(observer)
            stop(provider)
            stop(daemon)
            done.set()
            events.close()
            listener.join(timeout=2)
            for stream in streams:
                stream.close()
            peer.close()
    report.update(
        idle_seconds=args.idle_seconds,
        ticks_per_second=os.sysconf("SC_CLK_TCK"),
        native_sha256=hashlib.sha256(args.native.read_bytes()).hexdigest(),
        python_source_sha256=hashlib.sha256(
            args.python_source.read_bytes()
        ).hexdigest(),
        scope="Helper-owned CPU and RSS only; fixture bus/compositor/driver excluded; no wall/PSS evidence",
    )
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
