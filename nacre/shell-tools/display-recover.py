#!/usr/bin/env python3
"""Recreate renderer surfaces after output geometry/DPR changes, preserving safe UI flags."""

import argparse
from nacre_i18n import tr
import fcntl
import json
import os
import subprocess
import time
import sys
import tempfile
import uuid
from pathlib import Path

HOME = Path.home()


def main():
    p = argparse.ArgumentParser()
    p.add_argument("view", nargs="?")
    p.add_argument("--queue", action="store_true")
    a = p.parse_args()
    runtime = Path(os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}"))
    if a.queue:
        data = json.loads(sys.stdin.read(2 * 1024 * 1024))
        folder = runtime / "nacre-display-recovery"
        folder.mkdir(parents=True, exist_ok=True, mode=0o700)
        fd, name = tempfile.mkstemp(dir=folder, suffix=".json")
        with os.fdopen(fd, "w") as stream:
            json.dump(data, stream)
        os.chmod(name, 0o600)
        subprocess.run(
            [
                "systemd-run",
                "--user",
                "--collect",
                "--quiet",
                "--unit=nacre-display-fallback-" + uuid.uuid4().hex,
                sys.executable,
                str(Path(__file__).resolve()),
                "@" + name,
            ],
            check=True,
        )
        return
    saved_path = Path(a.view[1:]) if a.view and a.view.startswith("@") else None
    if saved_path:
        if (
            not saved_path.is_relative_to(runtime / "nacre-display-recovery")
            or saved_path.is_symlink()
        ):
            raise ValueError(
                tr(
                    "helper.display-recover.invalid-private-display-snapshot",
                    "Invalid private display snapshot",
                )
            )
        view = json.loads(saved_path.read_text())
    else:
        view = json.loads(a.view or "{}")
    with (runtime / "nacre-display-recovery.lock").open("w") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            return
        try:
            subprocess.run(
                [str(Path(__file__).with_name("nacre-helper")), "lock-config"],
                check=True,
                capture_output=True,
                timeout=4,
            )
        except (OSError, subprocess.SubprocessError):
            print(
                "Could not refresh lock layout; preserving the previous configuration",
                file=sys.stderr,
            )
        subprocess.run(
            ["systemctl", "--user", "restart", "nacre-shell.service"],
            check=True,
            timeout=15,
        )
        ipc = [
            str(HOME / ".local/share/nacre/shell/bin/qs"),
            "-c",
            "nacre",
            "ipc",
            "call",
            "nacre",
        ]
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            result = subprocess.run(
                [*ipc, "state"], capture_output=True, text=True, timeout=2
            )
            try:
                ready = result.returncode == 0 and "launcherMode" in json.loads(
                    result.stdout
                )
            except ValueError:
                ready = False
            if ready:
                subprocess.run(
                    [*ipc, "restoreViews", json.dumps(view)],
                    check=True,
                    capture_output=True,
                    timeout=3,
                )
                if saved_path:
                    saved_path.unlink(missing_ok=True)
                if view.get("dashboard") and view.get("tab") == 4:
                    subprocess.run(
                        [*ipc[:-1], "settingsView", "open", "displays"],
                        capture_output=True,
                        timeout=3,
                    )
                return
            time.sleep(0.2)
        raise RuntimeError(
            tr(
                "helper.display-recover.display-renderer-did-not-become-ready",
                "Display renderer did not become ready",
            )
        )


if __name__ == "__main__":
    main()
