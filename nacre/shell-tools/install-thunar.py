#!/usr/bin/env python3
"""Build only the app-scoped style module for the pacman-managed Thunar."""

import json
from pathlib import Path
import subprocess

HOME = Path.home()
DEST = HOME / ".local/share/nacre/thunar-style"


def install():
    subprocess.run(
        ["pacman", "-Q", "thunar", "xfconf", "exo", "libxfce4ui", "libgtop"], check=True
    )
    DEST.mkdir(parents=True, exist_ok=True)
    flags = subprocess.check_output(
        ["pkg-config", "--cflags", "--libs", "gtk+-3.0"], text=True
    ).split()
    output = DEST / "nacre-thunar-theme.so"
    candidate = output.with_suffix(".tmp.so")
    subprocess.run(
        [
            "cc",
            "-shared",
            "-fPIC",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-o",
            str(candidate),
            str(Path(__file__).with_name("thunar-theme.c")),
            *flags,
        ],
        check=True,
    )
    candidate.replace(output)
    launcher = HOME / ".local/share/applications/nacre-thunar.desktop"
    launcher.parent.mkdir(parents=True, exist_ok=True)
    launcher.write_text(
        "[Desktop Entry]\nType=Application\nName=Thunar Files\n"
        "Comment=Browse files with Nacre colors\nIcon=system-file-manager\n"
        "Exec=" + str(HOME / ".local/bin/nacre-shell") + " thunar %U\n"
        "Terminal=false\nDBusActivatable=false\nMimeType=inode/directory;\nCategories=System;FileManager;\n"
    )

    legacy = launcher.with_name("nacre-thunar-legacy.desktop")
    if legacy.exists():
        legacy.write_text(launcher.read_text() + "NoDisplay=true\n")
        current = subprocess.check_output(
            ["xdg-mime", "query", "default", "inode/directory"], text=True
        ).strip()
        if current == legacy.name:
            subprocess.run(
                ["xdg-mime", "default", launcher.name, "inode/directory"], check=True
            )
    marker = HOME / ".local/state/nacre/thunar-default.json"
    if not marker.exists():
        before = subprocess.check_output(
            ["xdg-mime", "query", "default", "inode/directory"], text=True
        ).strip()
        subprocess.run(
            ["xdg-mime", "default", "nacre-thunar.desktop", "inode/directory"],
            check=True,
        )
        marker.parent.mkdir(parents=True, exist_ok=True)
        marker.write_text(
            json.dumps({"before": before, "applied": "nacre-thunar.desktop"}) + "\n"
        )
        marker.chmod(0o600)


if __name__ == "__main__":
    install()
