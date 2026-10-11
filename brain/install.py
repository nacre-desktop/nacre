#!/usr/bin/env python3
"""Deploy only the current Brain, preserving browser profiles and other assistants."""

import ast
import datetime as dt
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import time
import urllib.request


ROOT = Path(__file__).resolve().parent
HOME = Path.home()
DEST = HOME / ".local/share/nacre/brain"


def main():
    for name in (
        "control.py",
        "auth.py",
        "discovery.py",
        "semantic.py",
        "semantic-client.py",
    ):
        ast.parse((ROOT / name).read_text())
    node = shutil.which("node")
    node_cmd = (
        [node]
        if node
        else (["nacre-ai-tools", "node"] if shutil.which("nacre-ai-tools") else [])
    )
    if not node_cmd:
        raise RuntimeError(
            "Node.js is required to validate Brain JavaScript before deployment"
        )
    subprocess.run([*node_cmd, "--check", str(ROOT / "web/app.js")], check=True)
    backup = (
        HOME
        / ".local/state/nacre/brain/backups"
        / ("brain-" + dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ"))
    )
    backup.mkdir(parents=True, mode=0o700)
    existing = DEST
    if existing.exists():
        shutil.copytree(
            existing, backup / "source", ignore=shutil.ignore_patterns("__pycache__")
        )
    subprocess.run(
        [
            "systemctl",
            "--user",
            "stop",
            "nacre-brain.service",
            "nacre-brain-sync.timer",
            "nacre-brain-check.timer",
            "nacre-brain-sync.service",
            "nacre-brain-check.service",
        ],
        capture_output=True,
    )
    # Old servers may predate the service. Match only this exact program's serve argv.
    for process in Path("/proc").iterdir():
        if not process.name.isdigit():
            continue
        try:
            if process.stat().st_uid != os.getuid():
                continue
            args = (process / "cmdline").read_bytes().split(b"\0")
            if (
                any(str(path / "control.py").encode() in args for path in (DEST,))
                and b"serve" in args
            ):
                os.kill(int(process.name), signal.SIGTERM)
        except (OSError, ProcessLookupError):
            pass
    DEST.mkdir(parents=True, exist_ok=True)
    for name in ("shell", "themes", "install.py", "wallpaper.py", "logo.svg"):
        old = DEST / name
        if old.is_dir():
            shutil.rmtree(old)
        elif old.exists():
            old.unlink()
    for name in (
        "control.py",
        "auth.py",
        "discovery.py",
        "semantic.py",
        "semantic-client.py",
    ):
        shutil.copy2(ROOT / name, DEST / name)
    shutil.copytree(ROOT / "web", DEST / "web", dirs_exist_ok=True)
    shutil.copytree(ROOT.parent / "ai/brain", DEST / "templates", dirs_exist_ok=True)
    for name in ("nacre-brain-ui",):
        wrapper = HOME / ".local/bin" / name
        wrapper.parent.mkdir(parents=True, exist_ok=True)
        if wrapper.is_symlink():
            wrapper.unlink()
        wrapper.write_text((ROOT / "launch.sh").read_text())
        wrapper.chmod(0o755)
    service = HOME / ".config/systemd/user/nacre-brain.service"
    service.parent.mkdir(parents=True, exist_ok=True)
    service.write_text((ROOT / "brain.service").read_text())
    subprocess.run(["systemctl", "--user", "daemon-reload"], check=True)
    subprocess.run(
        [
            "systemctl",
            "--user",
            "enable",
            "--now",
            "nacre-brain-sync.timer",
            "nacre-brain-check.timer",
        ],
        check=True,
    )
    subprocess.run(["systemctl", "--user", "enable", "--now", service.name], check=True)
    for _ in range(50):
        try:
            with urllib.request.urlopen(
                "http://127.0.0.1:17843/api/health", timeout=1
            ) as response:
                if json.load(response).get("ready"):
                    print("Brain ready. Private source backup:", backup)
                    return
        except OSError:
            pass
        time.sleep(0.2)
    raise RuntimeError(
        "Brain did not become ready; restore the source backup and restart its service"
    )


if __name__ == "__main__":
    main()
