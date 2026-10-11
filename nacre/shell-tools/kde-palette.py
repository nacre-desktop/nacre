#!/usr/bin/env python3
"""Publish the existing desktop palette as native KDE color roles."""

import configparser
import io
import os
from pathlib import Path
import tempfile


def save(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(dir=path.parent)
    try:
        with os.fdopen(fd, "w") as stream:
            stream.write(text)
        os.replace(temporary, path)
    finally:
        Path(temporary).unlink(missing_ok=True)


def read(path):
    config = configparser.ConfigParser(interpolation=None, strict=False)
    config.optionxform = str
    if path.exists():
        config.read(path)
    return config


def publish(home, colors, icon_theme):
    home = Path(home)
    config = read(home / ".config/kdeglobals")
    sets = {
        "Window": ("surfaceContainer", "onSurface"),
        "View": ("surface", "onSurface"),
        "Button": ("surfaceContainerHigh", "onSurface"),
        "Selection": ("primary", "onPrimary"),
        "Tooltip": ("surfaceContainerHigh", "onSurface"),
        "Complementary": ("surfaceContainerLow", "onSurface"),
        "Header": ("surfaceContainer", "onSurface"),
        "Header][Inactive": ("surfaceContainer", "onSurface"),
    }
    for name, (background, foreground) in sets.items():
        section = "Colors:" + name
        if not config.has_section(section):
            config.add_section(section)
        values = {
            "BackgroundNormal": background,
            "BackgroundAlternate": "surfaceContainerLow",
            "ForegroundNormal": foreground,
            "ForegroundInactive": "onSurfaceVariant",
            "ForegroundActive": foreground,
            "ForegroundLink": "primary",
            "ForegroundVisited": "tertiary",
            "ForegroundNegative": "error",
            "ForegroundPositive": "success",
            "ForegroundNeutral": "secondary",
            "DecorationFocus": "primary",
            "DecorationHover": "secondary",
        }
        if name == "Selection":
            values["BackgroundAlternate"] = "primary"
            for key in values:
                if key.startswith("Foreground"):
                    values[key] = "onPrimary"
        for key, role in values.items():
            config[section][key] = "#" + colors.get(role, colors["primary"]).lstrip("#")
    for section in ["General", "Icons"]:
        if not config.has_section(section):
            config.add_section(section)
    config["General"]["ColorScheme"] = "Nacre"
    config["General"]["ColorSchemeHash"] = ""
    if icon_theme:
        config["Icons"]["Theme"] = icon_theme
    stream = io.StringIO()
    config.write(stream, space_around_delimiters=False)
    save(home / ".config/kdeglobals", stream.getvalue())
    scheme = configparser.ConfigParser(interpolation=None)
    scheme.optionxform = str
    for section in config.sections():
        if section.startswith("Colors:"):
            scheme[section] = dict(config[section])
    scheme["General"] = {"Name": "Nacre", "ColorScheme": "Nacre"}
    stream = io.StringIO()
    scheme.write(stream, space_around_delimiters=False)
    save(home / ".local/share/color-schemes/Nacre.colors", stream.getvalue())
