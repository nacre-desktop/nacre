#!/usr/bin/env python3
"""Prepare private lock presentation on desktop events, outside lock startup."""

import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile

ROOT = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("lockinfo", ROOT / "lock-info.py")
info = importlib.util.module_from_spec(spec)
spec.loader.exec_module(info)
spec = importlib.util.spec_from_file_location(
    "lockdashboard", ROOT / "lock-dashboard.py"
)
dashboard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(dashboard)


def atomic(path, text):
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    fd, name = tempfile.mkstemp(dir=path.parent)
    with os.fdopen(fd, "wb") as stream:
        stream.write(text.encode() if isinstance(text, str) else text)
    os.chmod(name, 0o600)
    os.replace(name, path)


def prepare(data, home=None):
    home = Path.home() if home is None else Path(home)
    ready = home / ".cache/nacre/lock-ready"
    settings = data.get("preferences", {})
    data = dict(data, batteryLabel=info.label("status", data, settings))
    for kind in ("weather", "media", "play-icon", "status"):
        atomic(ready / (kind + ".txt"), info.label(kind, data, settings))
    for private in (False, True):
        preferences = dict(settings, lockNotificationContents=private)
        name = "notifications-private" if private else "notifications-safe"
        atomic(ready / (name + ".txt"), info.label("notifications", data, preferences))
    fallback = home / ".local/share/nacre/branding/nacre-lock.png"
    # A valid path is ready before bounded network/art decoding begins.
    if not (ready / "art-path.txt").exists():
        atomic(ready / "art-path.txt", str(fallback))
    old_home, old_cache = info.HOME, info.CACHE
    try:
        info.HOME = home
        info.CACHE = home / ".cache/nacre/lock-widgets.json"
        art = Path(info.artwork(data)) if settings.get("lockMedia", True) else fallback
        if art.is_file():
            atomic(ready / "initial-art.png", art.read_bytes())
        atomic(ready / "art-path.txt", str(art))
        atomic(ready / "snapshot.json", json.dumps(data))
        colors = data.get("colors")
        if not colors:
            try:
                colors = json.loads(
                    (home / ".local/state/nacre/scheme.json").read_text()
                )["colours"]
            except (OSError, ValueError, KeyError):
                colors = json.loads((ROOT / "reference-style.json").read_text())[
                    "colours"
                ]
        wallpaper = data.get("wallpaper", "")
        if not wallpaper:
            try:
                wallpaper = (
                    (home / ".local/state/nacre/wallpaper/last.txt").read_text().strip()
                )
            except OSError:
                pass
        try:
            monitors = json.loads((home / ".cache/nacre/lock-outputs.json").read_text())
        except (OSError, ValueError):
            monitors = []
        dashboard.publish(data, colors, wallpaper, art, home, monitors)
        # Bounded private artwork cache; live images use new paths for reloads.
        cached = sorted(
            info.CACHE.parent.glob("lock-art-*.png"),
            key=lambda p: p.stat().st_mtime,
            reverse=True,
        )
        for stale in cached[12:]:
            stale.unlink()
    finally:
        info.HOME, info.CACHE = old_home, old_cache


if __name__ == "__main__":
    prepare(json.loads(sys.stdin.readline() or "{}"))
