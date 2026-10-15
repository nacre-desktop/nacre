#!/usr/bin/env python3
"""Measure the isolated persistent media server with existing CPU/RSS methods."""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import select
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


def read_packet(child, timeout=30):
    deadline = time.monotonic() + timeout
    buffer = getattr(child, "_media_buffer", b"")
    while b"\n" not in buffer:
        remaining = deadline - time.monotonic()
        if remaining <= 0 or not select.select([child.stdout], [], [], remaining)[0]:
            raise AssertionError("Media server response timed out")
        chunk = os.read(child.stdout.fileno(), 65536)
        if not chunk:
            raise AssertionError("Media server closed its response stream")
        buffer += chunk
    line, child._media_buffer = buffer.split(b"\n", 1)
    return json.loads(line)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline-tools", type=Path, required=True)
    parser.add_argument("--native", type=Path, required=True)
    parser.add_argument("--repeats", type=int, default=20)
    parser.add_argument("--idle-seconds", type=float, default=30)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--prime-wallpaper",
        action="store_true",
        help="Publish one private scene before sampling idle memory",
    )
    args = parser.parse_args()
    if args.repeats < 1 or args.idle_seconds <= 0:
        parser.error("positive repetitions and idle duration required")
    source = args.baseline_tools.resolve() / "wallpaper-media.py"
    native = args.native.resolve()
    report = dict(
        condition="busy machine",
        source_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),
        native_sha256=hashlib.sha256(native.read_bytes()).hexdigest(),
        method="benchmark-helpers.py child CPU; performance.py helper RSS and process ticks",
        workload="startup + preferences CPU; idle memory "
        + (
            "after one wallpaper publication"
            if args.prime_wallpaper
            else "after preferences only"
        ),
        clock_ticks_per_second=os.sysconf("SC_CLK_TCK"),
        results={},
    )
    with tempfile.TemporaryDirectory(prefix="nacre-media-benchmark-") as folder:
        base = Path(folder)

        def start(side, sequence):
            home = base / side / str(sequence)
            home.mkdir(parents=True)
            (home / "runtime").mkdir(mode=0o700)
            env = dict(
                os.environ,
                HOME=str(home),
                XDG_CONFIG_HOME=str(home / ".config"),
                XDG_DATA_HOME=str(home / ".local/share"),
                XDG_STATE_HOME=str(home / ".local/state"),
                XDG_CACHE_HOME=str(home / ".cache"),
                DBUS_SESSION_BUS_ADDRESS="unix:path=" + str(home / "absent-bus"),
                DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + str(home / "absent-system-bus"),
                NACRE_HELPER_BINARY=str(native),
                NACRE_HELPER_TOOLS=str(ROOT / "nacre/shell-tools"),
                NACRE_LANGUAGE="en",
                NACRE_COMPOSITOR="hyprland",
                NIRI_SOCKET=str(home / "absent-niri"),
                HYPRLAND_INSTANCE_SIGNATURE="absent-fixture",
                XDG_RUNTIME_DIR=str(home / "runtime"),
                PYTHONPATH=str(source.parent),
            )
            for name in (
                "DISPLAY",
                "WAYLAND_DISPLAY",
                "SESSION_MANAGER",
                "FONTCONFIG_FILE",
                "FONTCONFIG_PATH",
                "FONTCONFIG_SYSROOT",
            ):
                env.pop(name, None)
            command = (
                [sys.executable, str(source), "serve"]
                if side == "python"
                else [str(native), "wallpaper-media", "serve"]
            )
            child = subprocess.Popen(
                command,
                env=env,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                bufsize=0,
            )
            try:
                if read_packet(child) != {"ready": True}:
                    raise AssertionError("Invalid media server readiness")
                child.stdin.write(
                    (
                        json.dumps(
                            dict(action="preferences", value=dict(layout="hexagons"))
                        )
                        + "\n"
                    ).encode()
                )
                child.stdin.flush()
                response = read_packet(child)
                if (
                    response.get("code") != 0
                    or response["result"]["layout"] != "hexagons"
                ):
                    raise AssertionError("Invalid media preferences response")
                return child
            except BaseException:
                stop(child)
                raise

        def stop(child):
            if child.poll() is None:
                child.stdin.close()
                child.wait(timeout=5)
            child.stdout.close()
            child.stderr.close()
            if child.returncode:
                raise AssertionError("Media server did not exit cleanly")

        for side in ("python", "rust"):
            report["results"][side] = dict(calls=args.repeats, cpu_seconds=0.0)
        for sequence in range(args.repeats):
            for side in ("python", "rust"):
                before = sum(HELPERS.cpu_times())
                stop(start(side, sequence))
                report["results"][side]["cpu_seconds"] += (
                    sum(HELPERS.cpu_times()) - before
                )
        for side in ("python", "rust"):
            row = report["results"][side]
            row["startup_request_cpu_ms"] = row.pop("cpu_seconds") * 1000 / args.repeats
            child = start(side, "idle")
            try:
                if args.prime_wallpaper:
                    from PIL import Image

                    home = base / side / "idle"
                    poster = home / "scene.png"
                    Image.new("RGB", (64, 40), "#172b45").save(poster)
                    config = home / ".config/nacre/cli.json"
                    config.parent.mkdir(parents=True, exist_ok=True)
                    config.write_text(
                        json.dumps({"version": 1, "wallpaper": {"postHook": None}})
                    )
                    scheme = home / ".local/state/nacre/scheme.json"
                    scheme.parent.mkdir(parents=True, exist_ok=True)
                    data = json.loads(
                        (ROOT / "nacre/shell-tools/reference-style.json").read_text()
                    )
                    data.update(
                        name="dynamic",
                        mode="dark",
                        variant="tonalspot",
                        flavour="default",
                    )
                    scheme.write_text(json.dumps(data))
                    bridge = home / ".local/share/nacre/shell/bin/nacre_shell"
                    bridge.parent.mkdir(parents=True, exist_ok=True)
                    bridge.write_bytes(
                        (ROOT / "nacre/shell-tools/cli-bridge.sh").read_bytes()
                    )
                    bridge.chmod(0o700)
                    child.stdin.write(
                        (
                            json.dumps(dict(action="select", value=str(poster))) + "\n"
                        ).encode()
                    )
                    child.stdin.flush()
                    while True:
                        # Publication may stream a presentation before completion.
                        response = read_packet(child, 120)
                        if response.get("done"):
                            if response.get("code") != 0 or response["result"].get(
                                "path"
                            ) != str(poster):
                                raise AssertionError(
                                    f"Private {side} wallpaper publication failed: {response}"
                                )
                            break
                rows = snapshot(child.pid, detached=False)
                if len(rows) != 1:
                    raise AssertionError("Unexpected media server child processes")
                first = HELPERS.cpu_times(child)
                samples = []
                started = time.monotonic()
                deadline = started + args.idle_seconds
                while time.monotonic() < deadline:
                    sample = memory(child.pid)
                    if sample["swap_pss"] != 0:
                        raise AssertionError("Measured media server has swapped pages")
                    samples.append(sample["rss"] / 1048576)
                    time.sleep(min(0.5, max(0, deadline - time.monotonic())))
                elapsed = time.monotonic() - started
                last = HELPERS.cpu_times(child)
                cpu = sum(last) - sum(first)
                row.update(
                    helper_rss_mib=sum(samples) / len(samples),
                    idle_cpu_seconds=cpu,
                    idle_seconds=elapsed,
                    idle_cpu_percent=100 * cpu / elapsed,
                    process_count=len(rows),
                    python_process_count=int(side == "python"),
                )
            finally:
                stop(child)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
