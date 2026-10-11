#!/usr/bin/env python3
"""Launch with current display state and recover a validated desktop after bad updates."""

import json
import os
import shutil
import signal
import stat
import subprocess
import time
from pathlib import Path

HOME = Path.home()
ROOT = HOME / ".local/share/nacre/shell"
GOOD = ROOT / "source.good"
stopping = False
child = None


def environment():
    env = dict(os.environ)
    runtime = Path(env.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}"))
    for _ in range(100):
        selected = Path(env.get("WAYLAND_DISPLAY", ""))
        selected = selected if selected.is_absolute() else runtime / selected
        try:
            valid = stat.S_ISSOCK(selected.stat().st_mode)
        except OSError:
            valid = False
        sockets = [
            p for p in runtime.glob("wayland-*") if stat.S_ISSOCK(p.stat().st_mode)
        ]
        if valid or len(sockets) == 1:
            if not valid:
                env["WAYLAND_DISPLAY"] = sockets[0].name
            break
        time.sleep(0.2)
    else:
        raise RuntimeError("Wayland display is not ready")
    result = subprocess.run(
        ["hyprctl", "instances", "-j"], capture_output=True, text=True
    )
    if result.returncode == 0:
        instances = json.loads(result.stdout)
        if len(instances) == 1:
            env["HYPRLAND_INSTANCE_SIGNATURE"] = instances[0]["instance"]
    env.setdefault("XDG_CURRENT_DESKTOP", "Hyprland")
    env.setdefault("XDG_SESSION_TYPE", "wayland")
    return env


def health(env):
    try:
        result = subprocess.run(
            [
                str(ROOT / "bin/qs"),
                "-c",
                "nacre",
                "ipc",
                "call",
                "nacre",
                "state",
            ],
            env=env,
            capture_output=True,
            text=True,
            timeout=2,
        )
        value = json.loads(result.stdout) if result.returncode == 0 else {}
        return "reveal" in value and "launcherMode" in value
    except (ValueError, OSError, subprocess.TimeoutExpired):
        return False


def restore():
    if not (GOOD / "shell.qml").is_file():
        return False
    source = ROOT / "source"
    bad = ROOT / ("source.failed-" + time.strftime("%Y%m%dT%H%M%S"))
    if source.exists():
        source.rename(bad)
    shutil.copytree(GOOD, source)
    print("Recovered the last validated Nacre desktop source.", flush=True)
    return True


def stop(*_):
    global stopping
    stopping = True
    if child and child.poll() is None:
        child.terminate()


def main():
    global child
    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    env = environment()
    recovered = False
    while not stopping:
        child = subprocess.Popen([str(ROOT / "bin/qs"), "-c", "nacre", "-n"], env=env)
        ready = False
        deadline = time.monotonic() + 20
        while not stopping and child.poll() is None and time.monotonic() < deadline:
            if health(env):
                ready = True
                break
            time.sleep(0.2)
        if not ready and not stopping:
            if child.poll() is None:
                child.terminate()
                child.wait(timeout=5)
            if not recovered and restore():
                recovered = True
                continue
            raise SystemExit(1)
        code = child.wait()
        if stopping or code == 0:
            return
        # Runtime failures use systemd restart; startup failures can restore known-good code.
        raise SystemExit(code)


if __name__ == "__main__":
    main()
