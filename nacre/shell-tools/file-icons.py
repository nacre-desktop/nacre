#!/usr/bin/env python3
"""Small user icon overlay; Papirus supplies the artwork and app/MIME fallbacks."""

import colorsys
import os
import re
import tempfile
from pathlib import Path

SYSTEM_ICONS = Path("/usr/share/icons")


def family(color):
    rgb = [int(color.lstrip("#")[i : i + 2], 16) / 255 for i in (0, 2, 4)]
    hue, saturation, _ = colorsys.rgb_to_hsv(*rgb)
    hue *= 360
    if saturation < 0.16:
        return "grey"
    if saturation < 0.36 and 20 <= hue < 70:
        return "brown"
    for end, name in [
        (20, "red"),
        (45, "orange"),
        (70, "yellow"),
        (165, "green"),
        (190, "teal"),
        (207, "cyan"),
        (252, "blue"),
        (278, "indigo"),
        (307, "violet"),
        (342, "pink"),
        (360, "red"),
    ]:
        if hue < end:
            return name
    return "red"


def overlay_index(directories, fallback):
    text = (
        "[Icon Theme]\nName=Nacre Files\nComment=Palette-matched Papirus folders\n"
        f"Inherits={fallback},Papirus,Adwaita,hicolor\nDirectories="
        + ",".join(path for path, _ in directories)
        + "\n"
    )
    for path, size in directories:
        text += f"\n[{path}]\nSize={size}\nContext=Places\nType=Fixed\n"
    return text


def overlay_link(path, source):
    # A regular file is a user override; only our link entries may be repaired.
    if path.exists() and not path.is_symlink():
        return
    source = source.resolve()
    if path.is_symlink() and path.resolve() == source:
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(dir=path.parent)
    os.close(descriptor)
    Path(temporary).unlink()
    try:
        Path(temporary).symlink_to(source)
        os.replace(temporary, path)
    finally:
        Path(temporary).unlink(missing_ok=True)


def theme(home, primary, mode):
    home = Path(home)
    bases = [home / ".local/share/icons", SYSTEM_ICONS]
    fallback = "Papirus-Dark" if mode == "dark" else "Papirus"
    if not any((base / fallback / "index.theme").is_file() for base in bases):
        return ""
    # Prefer the updated system artwork over legacy user copies.
    source = next(
        (
            base / "Papirus"
            for base in reversed(bases)
            if (base / "Papirus/index.theme").is_file()
        ),
        None,
    )
    if source is None:
        return fallback
    color = family(primary)
    name = "Nacre-Papirus-v3-" + color + "-" + mode
    target = bases[0] / name
    index = target / "index.theme"
    previous = index.read_text() if index.exists() else ""
    if previous:
        sizes = [
            int(size)
            for size in re.findall(r"(?m)^\[(32|48|64)x\1/places\]$", previous)
        ]
        known = overlay_index(
            [(f"{size}x{size}/places", size) for size in sizes], fallback
        )
        if previous != known:
            return name  # A customized index belongs to the user, not the cache repair.
    directories = []
    prefix = "folder-" + color
    for size in [32, 48, 64]:
        folder = source / f"{size}x{size}/places"
        if not (folder / (prefix + ".svg")).exists():
            continue
        relative = f"{size}x{size}/places"
        destination = target / relative
        destination.mkdir(parents=True, exist_ok=True)
        for icon in folder.glob(prefix + "*.svg"):
            if icon.stem != prefix and not icon.stem.startswith(prefix + "-"):
                continue
            alias = destination / ("folder" + icon.name[len(prefix) :])
            overlay_link(alias, icon)
        # KDE asks for the MIME name, while GTK usually asks for folder.
        for alias_name, source_name in {
            "inode-directory.svg": "folder.svg",
            "user-desktop.svg": "folder-desktop.svg",
            "desktop.svg": "folder-desktop.svg",
        }.items():
            source_icon = destination / source_name
            alias_icon = destination / alias_name
            if source_icon.exists():
                overlay_link(alias_icon, source_icon)
        directories.append((relative, size))
    if not directories:
        return fallback
    content = overlay_index(directories, fallback)
    if content != previous:
        index.write_text(content)
    return name
