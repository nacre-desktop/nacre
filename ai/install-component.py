#!/usr/bin/env python3
"""Install optional AI code; preserve accounts, conversation runtimes and workers."""

import datetime as dt
import importlib.util
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
HOME = Path.home()


def main():
    target = HOME / ".local/share/nacre/ai/application"
    backup = (
        HOME
        / ".local/state/nacre/ai/backups"
        / dt.datetime.now(dt.timezone.utc).strftime("component-%Y%m%dT%H%M%S%fZ")
    )
    if target.exists():
        shutil.copytree(
            target, backup / "application", ignore=shutil.ignore_patterns("__pycache__")
        )
    shutil.copytree(
        ROOT / "ai",
        target / "ai",
        dirs_exist_ok=True,
        ignore=shutil.ignore_patterns("tests", "__pycache__"),
    )
    binaries = target / "bin"
    binaries.mkdir(parents=True, exist_ok=True)
    spec = importlib.util.spec_from_file_location(
        "ai_installer", ROOT / "ai/install.py"
    )
    installer = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(installer)
    for source in (ROOT / "bin").glob("nacre-ai*"):
        if not source.is_file():
            continue
        destination = binaries / source.name
        shutil.copy2(source, destination)
        link = HOME / ".local/bin" / source.name
        link.parent.mkdir(parents=True, exist_ok=True)
        if link.is_symlink() and link.resolve().is_relative_to(
            HOME / ".local/share/nacre/ai/conversation-runtime"
        ):
            if source.name in ("nacre-ai", "nacre-ai-chat", "nacre-ai-claude"):
                installer.install_launcher(ROOT, HOME, apply=True, name=source.name)
            continue
        if link.exists() and not link.is_symlink():
            if link.read_bytes() != source.read_bytes():
                backup.mkdir(parents=True, exist_ok=True)
                shutil.copy2(link, backup / source.name)
            link.unlink()
        elif link.is_symlink():
            if link.resolve() != destination:
                raise RuntimeError(
                    "Unrecognized AI helper link preserved: " + str(link)
                )
            continue
        link.symlink_to(destination)
    service = HOME / ".config/systemd/user/nacre-sidebar-ai.service"
    service.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(ROOT / "nacre/shell-tools/nacre-sidebar-ai.service", service)
    subprocess.run(["systemctl", "--user", "daemon-reload"], check=True)
    print(
        "AI code installed. Existing accounts/workers preserved; provider setup is separate."
    )


if __name__ == "__main__":
    main()
