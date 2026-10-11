#!/usr/bin/env python3
"""Opt-in target-host check of the launcher routes and real Wayland Escape events."""

import argparse
import json
from pathlib import Path
import shutil
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--keyboard",
        default=shutil.which("wtype"),
        help="wtype executable; may be an isolated temporary build",
    )
    parser.add_argument(
        "--dismiss-hover",
        action="store_true",
        help="allow release checks to close transient dashboard/OSD hover panels",
    )
    args = parser.parse_args()
    if not args.keyboard:
        raise SystemExit(
            "Provide wtype to send Wayland key events; headless Qt tests cannot check compositor focus."
        )
    home = Path.home()
    ipc = [
        str(home / ".local/share/nacre/shell/bin/qs"),
        "-c",
        "nacre",
        "ipc",
        "call",
        "nacre",
    ]

    def state():
        return json.loads(subprocess.check_output([*ipc, "state"], text=True))

    initial = state()
    blockers = (
        ("launcher", "session", "left")
        if args.dismiss_hover
        else ("launcher", "session", "left", "dashboard", "osd")
    )
    if any(initial.get(k) for k in blockers):
        raise SystemExit(
            "Close shell panels first; this check must not disturb an existing view or draft."
        )
    if args.dismiss_hover:
        subprocess.run([*ipc, "close"], check=True)
    try:
        for action, mode in (("launcher", "apps"), ("wallpaper", "wallpaper")):
            subprocess.run([str(home / ".local/bin/nacre-shell"), action], check=True)
            time.sleep(0.6)
            opened = state()
            if not opened["launcher"] or opened["launcherMode"] != mode:
                raise RuntimeError(action + " closed during focus acquisition")
            subprocess.run([args.keyboard, "-k", "Escape"], check=True)
            time.sleep(0.3)
            if state()["launcher"]:
                raise RuntimeError(
                    action + " did not receive Escape before a mouse click"
                )
            print(action + ": opened, remained open, and received Escape")
    finally:
        subprocess.run([*ipc, "close"], check=True)


if __name__ == "__main__":
    main()
