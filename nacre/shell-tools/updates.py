#!/usr/bin/env python3
"""Read-only cached update count; refreshing never upgrades or modifies packages."""

import argparse
from nacre_i18n import tr
import fcntl
import json
import sys
import os
import shutil
import subprocess
import time
from pathlib import Path

CACHE = Path.home() / ".cache/nacre/updates.json"


def needs_host_runtime():
    """Package tools need real user IDs, including pacman's download user."""
    mapping = Path("/proc/self/uid_map")
    return mapping.is_file() and mapping.read_text().split() != ["0", "0", "4294967295"]


def package_command(command):
    if needs_host_runtime():
        return [
            "systemd-run",
            "--user",
            "--wait",
            "--pipe",
            "--collect",
            "--quiet",
            "--",
            *command,
        ]
    return command


def count_updates():
    commands = (
        [["checkupdates"]] if shutil.which("checkupdates") else [["pacman", "-Qu"]]
    )
    helper = next((x for x in ("paru", "yay") if shutil.which(x)), None)
    if helper:
        commands.append([helper, "-Qua"])
    total = 0
    for command in commands:
        if not shutil.which(command[0]):
            continue
        result = subprocess.run(
            package_command(command), capture_output=True, text=True, timeout=90
        )
        allowed = (0, 2) if command[0] == "checkupdates" else (0, 1)
        if result.returncode not in allowed or (
            result.returncode == 1 and result.stderr.strip()
        ):
            raise RuntimeError(
                tr("helper.updates.update-check-failed", "Update check failed")
            )
        total += len([line for line in result.stdout.splitlines() if line.strip()])
    return {"text": str(total), "count": total, "checked": time.time()}


def preflight():
    command = ["checkupdates"] if shutil.which("checkupdates") else ["pacman", "-Qu"]
    result = subprocess.run(
        package_command(command), capture_output=True, text=True, timeout=90
    )
    if result.returncode not in (0, 2) and not (
        command[0] == "pacman" and result.returncode == 1 and not result.stderr.strip()
    ):
        raise RuntimeError(
            tr(
                "helper.updates.update-preflight-failed-value0",
                "Update preflight failed: {value0}",
                {"value0": result.stderr.strip()},
            )
        )
    return {}


def refresh():
    CACHE.parent.mkdir(parents=True, exist_ok=True)
    with CACHE.with_suffix(".lock").open("w") as guard:
        try:
            fcntl.flock(guard, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            return
        data = count_updates()
        temp = CACHE.with_suffix(".next")
        temp.write_text(json.dumps(data))
        os.replace(temp, CACHE)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--refresh", action="store_true")
    parser.add_argument("--preflight", action="store_true")
    parser.add_argument("--needs-host", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.needs_host:
        raise SystemExit(0 if needs_host_runtime() else 1)
    if args.preflight:
        preflight()
        return
    if args.refresh:
        refresh()
        return
    try:
        data = json.loads(CACHE.read_text())
    except (OSError, ValueError):
        data = {"text": "0", "count": 0, "checked": 0}
    print(json.dumps(data), flush=True)
    if time.time() - data.get("checked", 0) > 1800:
        subprocess.Popen(
            [sys.executable, str(Path(__file__).resolve()), "--refresh"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            start_new_session=True,
        )


if __name__ == "__main__":
    helper = Path(
        os.environ.get("NACRE_HELPER_BINARY", Path(__file__).with_name("nacre-helper"))
    )
    if not helper.is_file():
        raise SystemExit("nacre-helper is not installed beside updates.py")
    os.execv(helper, [str(helper), "updates", *sys.argv[1:]])
