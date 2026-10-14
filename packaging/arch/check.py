#!/usr/bin/env python3
"""Run package checks on a private bus and clean up its asynchronous FUSE mounts."""

import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time


def mounts_below(directory, mountinfo=Path("/proc/self/mountinfo")):
    result = []
    for line in mountinfo.read_text().splitlines():
        fields = line.split()
        if len(fields) < 6:
            continue
        target = Path(
            re.sub(r"\\([0-7]{3})", lambda match: chr(int(match[1], 8)), fields[4])
        )
        if target == directory or target.is_relative_to(directory):
            result.append(str(target))
    return result


def cleanup(directory, timeout=15):
    deadline = time.monotonic() + timeout
    while True:
        mounts = mounts_below(directory)
        if not mounts:
            # Never traverse a FUSE filesystem if a service exits between the
            # mount check and removal. A late portal unmount can race GNU rm.
            result = subprocess.run(
                ["rm", "-rf", "--one-file-system", "--", str(directory)],
                capture_output=True,
                text=True,
            )
            if result.returncode == 0:
                return
        if time.monotonic() >= deadline:
            raise RuntimeError(
                "Private package-check runtime did not close cleanly: "
                + str(directory)
                + "; mounts="
                + repr(mounts)
            )
        time.sleep(0.1)


def main():
    if len(sys.argv) != 2:
        raise SystemExit("Usage: check.py PATH_TO_CHECK_PYTHON")
    runtime = Path(tempfile.mkdtemp(prefix="nacre-package-check-"))
    try:
        result = subprocess.run(
            ["dbus-run-session", "--", "xvfb-run", "-a", sys.argv[1], "tools/check.py"],
            env={**os.environ, "XDG_RUNTIME_DIR": str(runtime), "CI": "1"},
            stdin=subprocess.DEVNULL,
        )
        return result.returncode
    finally:
        # D-Bus is already closed; gvfs/document-portal may detach shortly after
        # dbus-run-session exits. Wait only for this fresh, mode-0700 directory.
        cleanup(runtime)


if __name__ == "__main__":
    raise SystemExit(main())
