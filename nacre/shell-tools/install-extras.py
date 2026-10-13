#!/usr/bin/env python3
"""Own the native shortcut routes without replacing personal compositor config."""

from pathlib import Path
from nacre_i18n import tr
import datetime as dt
import subprocess

import compositor

ROOT = Path(__file__).resolve().parent
HOME = Path.home()


def put(path, text):
    if path.exists() and path.read_text() == text:
        return False
    if path.exists():
        backup = (
            HOME
            / ".local/state/nacre/shell/backups"
            / (
                "extras-"
                + dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
            )
            / path.relative_to(HOME)
        )
        backup.parent.mkdir(parents=True, exist_ok=True)
        backup.write_text(path.read_text())
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)
    return True


def install():
    subprocess.run(
        ["python3", str(ROOT / "lock-prepare.py")], input="{}\n", text=True, check=True
    )
    subprocess.run(["python3", str(ROOT / "install-thunar.py")], check=True)
    changed = put(
        HOME / ".config/nacre/shortcuts.lua",
        (ROOT / "shortcuts.lua").read_text(),
    )
    main = HOME / ".config/hypr/hyprland.lua"
    text = main.read_text()
    marker = "-- Nacre native desktop shortcuts"
    if marker not in text or 'load_private("shortcuts")' not in text:
        raise RuntimeError(
            tr(
                "helper.install-extras.managed-shortcuts-loader-missing-run-install-sh-apply-from-the-re",
                "Managed shortcuts loader missing; run ./install.sh --apply from the repository",
            )
        )

    if changed and compositor.session_id():
        compositor.reload(timeout=None)


if __name__ == "__main__":
    install()
