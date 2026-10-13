#!/usr/bin/env python3
"""Repeat the full check concurrently, with a private display and per-run state."""

import argparse
from concurrent.futures import ThreadPoolExecutor
import json
import os
from pathlib import Path
import select
import signal
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.jobs < 1 or args.rounds < 1:
        parser.error("jobs and rounds must be positive")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="nacre-load-") as folder:
        read_fd, write_fd = os.pipe()
        with (output / "display.log").open("w") as log:
            display = subprocess.Popen(
                [
                    "Xvfb",
                    "-displayfd",
                    str(write_fd),
                    "-screen",
                    "0",
                    "1600x1000x24",
                    "-nolisten",
                    "tcp",
                    "-ac",
                ],
                stdin=subprocess.DEVNULL,
                stdout=log,
                stderr=subprocess.STDOUT,
                pass_fds=(write_fd,),
                start_new_session=True,
            )
            os.close(write_fd)
            try:
                if not select.select([read_fd], [], [], 10)[0]:
                    raise TimeoutError("Private display did not start")
                x_display = ":" + os.read(read_fd, 64).decode().strip()
                os.close(read_fd)
                results = []
                for round_number in range(1, args.rounds + 1):

                    def run(worker, round_number=round_number):
                        home = Path(folder) / f"round-{round_number}-{worker}"
                        home.mkdir()
                        runtime = home / "runtime"
                        runtime.mkdir(mode=0o700)
                        env = dict(os.environ)
                        for key in (
                            "WAYLAND_DISPLAY",
                            "NIRI_SOCKET",
                            "HYPRLAND_INSTANCE_SIGNATURE",
                            "DBUS_SESSION_BUS_ADDRESS",
                            "QML_IMPORT_PATH",
                            "QML2_IMPORT_PATH",
                            "QT_LOGGING_RULES",
                        ):
                            env.pop(key, None)
                        env.update(
                            HOME=str(home),
                            XDG_RUNTIME_DIR=str(runtime),
                            DISPLAY=x_display,
                            QT_QPA_PLATFORM="offscreen",
                        )
                        name = f"round-{round_number}-{worker}.log"
                        started = time.monotonic()
                        with (output / name).open("w") as log:
                            process = subprocess.Popen(
                                [
                                    "dbus-run-session",
                                    "--",
                                    sys.executable,
                                    str(ROOT / "tools/check.py"),
                                ],
                                cwd=ROOT,
                                env=env,
                                stdin=subprocess.DEVNULL,
                                stdout=log,
                                stderr=subprocess.STDOUT,
                                start_new_session=True,
                            )
                            try:
                                code = process.wait(timeout=600)
                            except subprocess.TimeoutExpired:
                                os.killpg(process.pid, signal.SIGKILL)
                                process.wait()
                                code = 124
                        return dict(
                            round=round_number,
                            worker=worker,
                            code=code,
                            seconds=round(time.monotonic() - started, 2),
                            log=name,
                        )

                    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
                        rows = list(pool.map(run, range(1, args.jobs + 1)))
                    results.extend(rows)
                    (output / "results.json").write_text(
                        json.dumps(results, indent=2) + "\n"
                    )
                    print(
                        f"Round {round_number}: "
                        + ", ".join(
                            f"worker {row['worker']} code {row['code']} in {row['seconds']}s"
                            for row in rows
                        ),
                        flush=True,
                    )
                if any(row["code"] for row in results):
                    raise SystemExit(1)
            finally:
                if display.poll() is None:
                    os.killpg(display.pid, signal.SIGTERM)
                    display.wait(timeout=5)


if __name__ == "__main__":
    main()
