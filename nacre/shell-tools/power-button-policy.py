#!/usr/bin/env python3
"""Retire only the recognized private display-blanking power binding."""

import datetime as dt
import os
from pathlib import Path
import tempfile

LEGACY = """-- Run after release/input processing so this key does not immediately wake DPMS.
hl.bind("XF86PowerOff", hl.dsp.exec_cmd("sleep 0.2; ~/.config/hypr/scripts/hyprctl-lua.sh dpms disable"), {
    release = true,
    locked = true,
})
"""


def prepare(home=None):
    home = Path.home() if home is None else Path(home)
    targets = (
        home / ".config/nacre/host.lua",
        home / ".config/hypr/conf/manual-power.lua",
    )
    changes = []
    for path in targets:
        if not path.is_file():
            continue
        text = path.read_text()
        if "XF86PowerOff" not in text:
            continue
        if LEGACY not in text or path.is_symlink():
            raise RuntimeError(
                "Private power binding differs; reconcile "
                + str(path)
                + " before deployment"
            )
        clean = text.replace(LEGACY, "").replace(
            "-- Local preference: manual display blanking, keyboard/mouse wake, no idle lock.",
            "-- Host display wake preference. Managed key bindings own power-button locking.",
        )
        changes.append((path, text, clean))
    if not changes:
        return False
    backup = (
        home
        / ".local/state/nacre/backups"
        / dt.datetime.now(dt.timezone.utc).strftime("power-button-%Y%m%dT%H%M%S%fZ")
    )
    for path, before, after in changes:
        saved = backup / path.relative_to(home)
        saved.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        saved.write_text(before)
        saved.chmod(0o600)
        fd, temporary = tempfile.mkstemp(dir=path.parent)
        with os.fdopen(fd, "w") as stream:
            stream.write(after)
        os.chmod(temporary, 0o600)
        os.replace(temporary, path)
    return True


if __name__ == "__main__":
    if prepare():
        import subprocess

        subprocess.run(["hyprctl", "reload"], check=True, capture_output=True)
