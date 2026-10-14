#!/usr/bin/env python3
"""Compile native integrations and seed offline assets in an isolated build home."""

import argparse
import importlib.util
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys


def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def build(root, output, sources):
    tools = root / "nacre/shell-tools"
    sys.path.insert(0, str(tools))
    sys.path.insert(0, str(root / "nacre/shell-cli/src"))
    output.mkdir(parents=True, exist_ok=True)
    (output / "revision").write_text(
        subprocess.check_output(
            ["git", "-C", str(root), "rev-parse", "HEAD"], text=True
        )
    )
    native = output / "native"
    native.mkdir(exist_ok=True)
    helper = Path(
        subprocess.check_output(
            ["bash", str(root / "tools/build-helper.sh")], text=True
        ).strip()
    )
    shutil.copy2(helper, native / "nacre-helper")
    for name in ("dolphin-style/styles", "thunar-style"):
        (native / name).mkdir(parents=True, exist_ok=True)
    compiler_flags = shlex.split(os.environ.get("CFLAGS", ""))
    linker_flags = shlex.split(os.environ.get("LDFLAGS", ""))
    gtk_flags = shlex.split(
        subprocess.check_output(
            ["pkg-config", "--cflags", "--libs", "gtk+-3.0"], text=True
        )
    )
    subprocess.run(
        [
            "cc",
            *compiler_flags,
            "-shared",
            "-fPIC",
            str(tools / "thunar-theme.c"),
            "-o",
            str(native / "thunar-style/nacre-thunar-theme.so"),
            *linker_flags,
            *gtk_flags,
        ],
        check=True,
    )
    dolphin = output / "dolphin"
    dolphin.mkdir(exist_ok=True)
    for name in ("dolphin-style.cpp", "dolphin-style.json"):
        shutil.copy2(tools / name, dolphin / name)
    subprocess.run(
        [
            "/usr/lib/qt6/moc",
            str(dolphin / "dolphin-style.cpp"),
            "-o",
            str(dolphin / "dolphin-style.moc"),
        ],
        check=True,
    )
    qt_flags = shlex.split(
        subprocess.check_output(
            ["pkg-config", "--cflags", "--libs", "Qt6Widgets"], text=True
        )
    )
    subprocess.run(
        [
            "c++",
            *shlex.split(os.environ.get("CXXFLAGS", "")),
            "-std=c++17",
            "-shared",
            "-fPIC",
            str(dolphin / "dolphin-style.cpp"),
            "-o",
            str(native / "dolphin-style/styles/libnacre-dolphin.so"),
            *linker_flags,
            *qt_flags,
            "-I/usr/include/KF6/KConfig",
            "-I/usr/include/KF6/KConfigCore",
            "-I/usr/include/KF6/KColorScheme",
            "-lKF6ConfigCore",
            "-lKF6ColorScheme",
        ],
        check=True,
    )
    fonts = output / "fonts"
    fonts.mkdir(exist_ok=True)
    for group in json.loads((tools / "font-assets.json").read_text())["groups"]:
        if group["default"]:
            for row in group["files"]:
                shutil.copy2(sources / row["name"], fonts / row["name"])
    home = output / "defaults"
    home.mkdir(exist_ok=True)
    # Seed the same reference palette; no host preferences, services or accounts.
    state = home / ".local/state/nacre"
    state.mkdir(parents=True, exist_ok=True)
    shutil.copy2(tools / "reference-style.json", state / "scheme.json")
    for version in ("3.0", "4.0"):
        settings = home / f".config/gtk-{version}/settings.ini"
        settings.parent.mkdir(parents=True, exist_ok=True)
        settings.write_text(
            "[Settings]\ngtk-theme-name=Adwaita-dark\ngtk-icon-theme-name=Papirus-Dark\ngtk-application-prefer-dark-theme=1\n"
        )
    load(tools / "classic-state.py", "package_palette").apply_palette(home, live=False)
    lock = load(tools / "lock-prepare.py", "package_lock")
    lock.prepare({}, home)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("sources", type=Path)
    args = parser.parse_args()
    build(args.root.resolve(), args.output.resolve(), args.sources.resolve())
