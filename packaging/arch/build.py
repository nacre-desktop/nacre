#!/usr/bin/env python3
"""Compile native integrations and seed offline assets in an isolated build home."""

import argparse
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys


def seed_defaults(root, home, helper):
    """Generate seeds with the candidate binary, without the builder's session."""
    tools = root / "nacre/shell-tools"
    home.mkdir(parents=True, exist_ok=True)
    runtime = home / ".runtime"
    runtime.mkdir(mode=0o700, exist_ok=True)
    environment = {
        key: value
        for key, value in os.environ.items()
        if not key.startswith(("NACRE_", "NIRI_", "HYPR", "UWSM_", "XDG_", "DBUS_"))
        and key not in ("DISPLAY", "WAYLAND_DISPLAY", "NOTIFY_SOCKET", "PYTHONPATH")
    }
    environment.update(
        HOME=str(home),
        XDG_CONFIG_HOME=str(home / ".config"),
        XDG_STATE_HOME=str(home / ".local/state"),
        XDG_CACHE_HOME=str(home / ".cache"),
        XDG_DATA_HOME=str(home / ".local/share"),
        XDG_RUNTIME_DIR=str(runtime),
        DBUS_SESSION_BUS_ADDRESS="unix:path=" + str(runtime / "absent-session-bus"),
        DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + str(runtime / "absent-system-bus"),
        NACRE_HELPER_BINARY=str(helper),
        NACRE_HELPER_TOOLS=str(tools),
    )
    # Seed the same reference palette; no host preferences, services or accounts.
    state = home / ".local/state/nacre"
    state.mkdir(parents=True, exist_ok=True)
    shutil.copy2(tools / "reference-style.json", state / "scheme.json")
    wallpaper = state / "wallpaper/last.txt"
    wallpaper.parent.mkdir(parents=True, exist_ok=True)
    wallpaper.write_text(str(tools / "demo-wallpapers/forest.jpg") + "\n")
    for version in ("3.0", "4.0"):
        settings = home / f".config/gtk-{version}/settings.ini"
        settings.parent.mkdir(parents=True, exist_ok=True)
        settings.write_text(
            "[Settings]\ngtk-theme-name=Adwaita-dark\ngtk-icon-theme-name=Papirus-Dark\ngtk-application-prefer-dark-theme=1\n"
        )
    result = subprocess.run(
        [str(helper), "palette-api", "classic-state", "apply_palette"],
        input=json.dumps({"args": [{"$path": str(home)}, None, False], "kwargs": {}}),
        text=True,
        capture_output=True,
        env=environment,
        cwd=root,
        check=True,
        timeout=120,
    )
    if json.loads(result.stdout) != {"result": None}:
        raise RuntimeError(
            "Unexpected native palette preparation result: " + result.stdout
        )
    # This remaining lock-data caller bridges directly to the native renderer.
    subprocess.run(
        [sys.executable, str(tools / "lock-prepare.py")],
        input="{}\n",
        text=True,
        env=environment,
        cwd=root,
        check=True,
        timeout=120,
    )
    runtime.rmdir()


def build(root, output, sources):
    tools = root / "nacre/shell-tools"
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
    seed_defaults(root, output / "defaults", native / "nacre-helper")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("sources", type=Path)
    args = parser.parse_args()
    build(args.root.resolve(), args.output.resolve(), args.sources.resolve())
