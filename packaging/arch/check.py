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


PRODUCT_BINARIES = ("nacre-shell", "bead-wallpaper-video", "nacre-wallpaper-decode")


def verify_ui_shell(bundle):
    bundle = Path(bundle)
    for name in PRODUCT_BINARIES:
        path = bundle / "bin" / name
        if not path.is_file() or path.is_symlink():
            raise RuntimeError(f"Missing regular UI shell executable: {path}")
        with path.open("rb") as executable:
            elf_magic = executable.read(4)
        if not path.stat().st_mode & 0o111 or elf_magic != b"\x7fELF":
            raise RuntimeError(f"Invalid UI shell executable: {path}")
    if not (bundle / "shell/palette.json").is_file():
        raise RuntimeError(f"Missing UI shell palette: {bundle / 'shell/palette.json'}")
    if not (bundle / "shell/bar").is_dir():
        raise RuntimeError(f"Missing UI shell bar bundle: {bundle / 'shell/bar'}")


def main():
    if len(sys.argv) != 3:
        raise SystemExit("Usage: check.py PATH_TO_CHECK_PYTHON UI_SHELL_BUNDLE")
    verify_ui_shell(sys.argv[2])
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
