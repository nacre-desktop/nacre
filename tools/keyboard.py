"""Seed a private keyboard override once from the system's XKB configuration."""

import json
import os
from pathlib import Path
import re
import subprocess


def system_keyboard():
    defaults = {"layout": "us", "model": "", "variant": "", "options": ""}
    try:
        result = subprocess.run(
            ["localectl", "--no-pager", "status"],
            env={**os.environ, "LC_ALL": "C"},
            capture_output=True,
            text=True,
            timeout=3,
        )
    except (OSError, subprocess.SubprocessError):
        return defaults
    if result.returncode:
        return defaults
    for key in defaults:
        match = re.search(
            r"^\s*X11 " + key.title() + r":\s*(.*?)\s*$", result.stdout, re.M
        )
        if (
            match
            and re.fullmatch(r"[a-zA-Z0-9_+:,.-]{1,200}", match[1])
            and match[1] != "n/a"
        ):
            defaults[key] = match[1]
    return defaults


def seed(home, settings=None):
    target = Path(home) / ".config/nacre/keyboard.lua"
    if target.exists() or target.is_symlink():
        return False
    values = system_keyboard() if settings is None else settings
    rows = [
        "-- Your system keyboard choice. Edit here; Nacre updates preserve this file."
    ]
    rows.append("hl.config({")
    rows.extend(
        '    ["input.kb_' + key + '"] = ' + json.dumps(value) + ","
        for key, value in values.items()
    )
    rows.append("})")
    target.parent.mkdir(parents=True, exist_ok=True)
    # Exclusive creation preserves an override created concurrently.
    try:
        with target.open("x") as stream:
            stream.write("\n".join(rows) + "\n")
    except FileExistsError:
        return False
    target.chmod(0o600)
    return True
