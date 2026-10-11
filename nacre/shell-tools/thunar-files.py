#!/usr/bin/env python3
"""Launch native Thunar with the app-scoped, live Nacre style."""

import fcntl
import json
import time
import os
from pathlib import Path
import subprocess
import sys

HOME = Path.home()
ROOT = Path(__file__).resolve().parent
RUNTIME = HOME / ".local/share/nacre/thunar-style"


def environment():
    env = os.environ.copy()
    for name in (
        "LD_LIBRARY_PATH",
        "QT_PLUGIN_PATH",
        "QML_IMPORT_PATH",
        "QML2_IMPORT_PATH",
    ):
        env.pop(name, None)
    env["PATH"] = ":".join(
        entry
        for entry in env.get("PATH", "/usr/bin:/bin").split(":")
        if "shell-runtime/usr/" not in entry
        and "palette-runtime/usr/" not in entry
        and "thunar-runtime/usr/" not in entry
    )
    return env


def setup_locked(env):
    # The distribution D-Bus service activates Xfconf on demand.
    marker = HOME / ".local/state/nacre/thunar-style.json"
    if marker.exists():
        return
    query = "/usr/bin/xfconf-query"
    # xfconf-query waits for the service to become ready through its D-Bus client.
    defaults = {
        "default-view": ("string", "ThunarIconView"),
        "last-view": ("string", "ThunarIconView"),
        "last-icon-view-zoom-level": ("string", "THUNAR_ZOOM_LEVEL_150_PERCENT"),
        "last-location-bar": ("string", "ThunarLocationButtons"),
        "last-side-pane": ("string", "ThunarShortcutsPane"),
        "last-menubar-visible": ("bool", "false"),
        "last-statusbar-visible": ("bool", "true"),
        "last-separator-position": ("int", "210"),
        "last-window-width": ("int", "1100"),
        "last-window-height": ("int", "760"),
        "misc-single-click": ("bool", "false"),
        "misc-thumbnail-mode": ("string", "THUNAR_THUMBNAIL_MODE_ONLY_LOCAL"),
        "misc-thumbnail-draw-frames": ("bool", "false"),
        "shortcuts-icon-size": ("string", "THUNAR_ICON_SIZE_24"),
    }
    before = {}
    for key, (kind, value) in defaults.items():
        result = subprocess.run(
            [query, "-c", "thunar", "-p", "/" + key],
            env=env,
            capture_output=True,
            text=True,
        )
        before[key] = result.stdout.strip() if result.returncode == 0 else None
        command = [
            query,
            "-c",
            "thunar",
            "-p",
            "/" + key,
            "--create",
            "--type",
            kind,
            "--set",
            value,
        ]
        for attempt in range(30):
            result = subprocess.run(
                command, env=env, capture_output=True, text=True, timeout=8
            )
            if result.returncode == 0:
                break
            if attempt == 29:
                raise RuntimeError(
                    result.stderr.strip() or "Unable to save Thunar preferences"
                )
            time.sleep(0.05)
    marker.parent.mkdir(parents=True, exist_ok=True)
    marker.write_text(
        json.dumps({"before": before, "applied": defaults}, indent=2) + "\n"
    )
    marker.chmod(0o600)


def setup(env):
    state = HOME / ".local/state/nacre"
    state.mkdir(parents=True, exist_ok=True)
    with (state / "thunar-setup.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        setup_locked(env)


def main():
    env = environment()
    setup(env)
    env["GTK_MODULES"] = str(RUNTIME / "nacre-thunar-theme.so")
    env["NACRE_THUNAR_STYLE"] = str(ROOT / "thunar.css")
    os.execve("/usr/bin/thunar", ["thunar", *(sys.argv[1:] or [str(HOME)])], env)


if __name__ == "__main__":
    main()
