"""Seed a private keyboard override once from the system's XKB configuration."""

import importlib.util
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
    if target.is_symlink():
        return False
    values = system_keyboard() if settings is None else settings
    spec = importlib.util.spec_from_file_location(
        "nacre_keyboard_compositor",
        Path(__file__).resolve().parents[1]
        / "nacre/shell-tools/nacre_native_compositor.py",
    )
    compositor = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(compositor)
    content = compositor.render_for_backend("hyprland", "render_keyboard", values)
    if target.exists():
        # Early packaged setup selected the *running* backend. Repair only its
        # exact generated KDL accidentally saved as Lua; never parse user Lua.
        previous = target.read_text()
        fields = re.findall(
            r'^            (layout|model|variant|options) ("(?:[^"\\]|\\.)*")$',
            previous,
            re.M,
        )
        if len(fields) != 4 or len(dict(fields)) != 4:
            return False
        saved = {key: json.loads(value) for key, value in fields}
        legacy = compositor.render_for_backend("niri", "render_keyboard", saved)
        if previous != legacy:
            return False
        content = compositor.render_for_backend("hyprland", "render_keyboard", saved)
        from configure import atomic

        atomic(target, content.encode(), 0o600)
        return True
    target.parent.mkdir(parents=True, exist_ok=True)
    # Exclusive creation preserves an override created concurrently.
    try:
        with target.open("x") as stream:
            stream.write(content)
    except FileExistsError:
        return False
    target.chmod(0o600)
    return True
