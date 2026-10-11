#!/usr/bin/env python3
"""Seed native Dolphin defaults once; preserve subsequent user edits."""

import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent


def install():
    subprocess.run(
        [
            "pacman",
            "-Q",
            "dolphin",
            "kio-extras",
            "qt6ct",
            "breeze",
            "papirus-icon-theme",
        ],
        check=True,
    )
    spec = importlib.util.spec_from_file_location(
        "dolphin_files", ROOT / "dolphin-files.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.setup()
    # Only this public-Qt style plugin is local; native Qt/Breeze remain pacman-owned.
    output = Path.home() / ".local/share/nacre/dolphin-style/styles/libnacre-dolphin.so"
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="nacre-dolphin-build-") as directory:
        build = Path(directory)
        for name in ("dolphin-style.cpp", "dolphin-style.json"):
            (build / name).write_bytes((ROOT / name).read_bytes())
        subprocess.run(
            [
                "/usr/lib/qt6/moc",
                str(build / "dolphin-style.cpp"),
                "-o",
                str(build / "dolphin-style.moc"),
            ],
            check=True,
        )
        flags = subprocess.check_output(
            ["pkg-config", "--cflags", "--libs", "Qt6Widgets"], text=True
        ).split()
        pending = output.with_suffix(".next.so")
        subprocess.run(
            [
                "c++",
                "-std=c++17",
                "-shared",
                "-fPIC",
                "-Wall",
                "-Wextra",
                "-Werror",
                str(build / "dolphin-style.cpp"),
                "-o",
                str(pending),
                *flags,
                "-I/usr/include/KF6/KConfig",
                "-I/usr/include/KF6/KConfigCore",
                "-I/usr/include/KF6/KColorScheme",
                "-lKF6ConfigCore",
                "-lKF6ColorScheme",
            ],
            check=True,
        )
        pending.replace(output)
    service = (
        Path.home()
        / ".local/share/dbus-1/services/org.freedesktop.FileManager1.service"
    )
    service.parent.mkdir(parents=True, exist_ok=True)
    content = (
        "[D-BUS Service]\nName=org.freedesktop.FileManager1\nExec="
        + str(Path.home() / ".local/bin/nacre-shell")
        + " dolphin --daemon\n"
    )
    if service.exists() and service.read_text() != content:
        backup = (
            Path.home()
            / ".local/state/nacre/backups/filemanager1-before-dolphin.service"
        )
        if not backup.exists():
            backup.parent.mkdir(parents=True, exist_ok=True)
            backup.write_bytes(service.read_bytes())
    service.write_text(content)
    # Rebuild KDE's application menu metadata without starting a Plasma session.
    subprocess.run(
        ["kbuildsycoca6", "--noincremental"],
        env={**os.environ, "XDG_MENU_PREFIX": "arch-"},
        check=True,
        capture_output=True,
    )
    old = Path.home() / ".local/share/applications/nacre-thunar.desktop"
    if old.is_file() and "nacre-shell thunar" in old.read_text():
        saved = (
            Path.home() / ".local/state/nacre/backups/retired-thunar-launcher.desktop"
        )
        if not saved.exists():
            saved.parent.mkdir(parents=True, exist_ok=True)
            saved.write_bytes(old.read_bytes())
        old.unlink()


if __name__ == "__main__":
    install()
