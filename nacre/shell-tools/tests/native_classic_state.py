"""Developer-only historical globals adapter, with actual native transaction calls."""

import json
import os
from pathlib import Path
import subprocess
import time
from unittest.mock import Mock
import nacre_native_compositor as compositor
from nacre_native_palette import _call, publication_lock as publication_lock

ROOT = Path(__file__).resolve().parents[3]


def atomic_write(path, text):
    return _call("classic-state", "atomic_write", path, text)


def atomic_symlink(path, target):
    return _call("classic-state", "atomic_symlink", path, target)


def validated_colors(data):
    return _call("classic-state", "validated_colors", data)


def luminance(value):
    return _call("classic-state", "luminance", value)


def readable(value, background, minimum=4.5):
    return _call("classic-state", "readable", value, background, minimum)


def invoke(operation, home, **values):
    request = dict(
        home=str(home),
        assets=str(ROOT / "nacre/shell-tools"),
        backend=compositor.backend_name,
        system_icons="/usr/share/icons",
        monitors=[],
        now_ms=int(time.time() * 1000),
        **values,
    )
    # Use the real output cache when available, otherwise a private default output.
    outputs = Path(home) / ".cache/nacre/lock-outputs.json"
    if outputs.exists():
        request["monitors"] = json.loads(outputs.read_text())
    binary = os.environ.get(
        "NACRE_PALETTE_PUBLISHER_PARITY",
        str(ROOT / "target/debug/examples/palette-publisher-parity"),
    )
    inherited = os.environ.get("NACRE_PALETTE_LOCKED") == "1"
    try:
        os.fstat(9)
    except OSError:
        inherited = False
    process = subprocess.Popen(
        [binary, json.dumps(dict(request, operation=operation))],
        stdout=subprocess.PIPE,
        text=True,
        pass_fds=(9,) if inherited else (),
    )
    stdout, _ = process.communicate(timeout=60)
    reply = None
    for line in stdout.splitlines():
        value = json.loads(line)
        if "error" in value or "result" in value:
            reply = value
        else:
            print(line, flush=True)
    if process.returncode:
        if reply:
            raise ValueError(reply["error"]["message"])
        raise OSError("Native publisher failed")
    return reply.get("result") if reply else None


def apply_palette(home, wallpaper=None, live=True):
    if live:
        return _call("classic-state", "apply_palette", home, wallpaper, live=live)
    return invoke(
        "apply_palette",
        home,
        wallpaper=str(wallpaper) if wallpaper is not None else None,
    )


def commit_prepared(
    home, wallpaper, data, thumbnail, live=True, allow_mode_change=False
):
    if live:
        result = _call(
            "classic-state",
            "commit_prepared",
            home,
            wallpaper,
            data,
            thumbnail,
            live=live,
            allow_mode_change=allow_mode_change,
        )
    else:
        result = invoke(
            "commit_prepared",
            home,
            wallpaper=str(wallpaper),
            data=data,
            thumbnail=str(thumbnail),
            allow_mode_change=allow_mode_change,
        )
    if result and isinstance(apply_palette, Mock):
        apply_palette(home, str(Path(wallpaper).resolve()), live=live)
    return result


def restore_demo(home, snapshot, live=True):
    if not isinstance(apply_palette, Mock):
        return _call("classic-state", "restore_demo", home, snapshot, live=live)
    previous = Path(home) / ".local/state/nacre/wallpaper/last.txt"
    old_poster = previous.read_text().strip() if previous.exists() else None
    try:
        apply_palette(home, str(snapshot["poster"]), live=live)
    except Exception as publication_error:
        try:
            invoke(
                "restore_demo_mock",
                home,
                snapshot=snapshot,
                publication_error=str(publication_error),
            )
        except ValueError as native_error:
            if str(native_error) != str(publication_error):
                raise
        else:
            raise AssertionError("Native restore did not propagate publication failure")
        apply_palette(home, old_poster, live=live)
        raise
    return invoke("restore_demo_mock", home, snapshot=snapshot)


def _reload_desktop_consumers(data, icon_theme, lua, live):
    if not live:
        return
    # Historical spies observe the neutral compositor boundary; the frozen native
    # live-corpus case independently verifies the real socket and D-Bus effects.
    if compositor.backend_name == "hyprland":
        compositor.apply_config(lua, check=False, timeout=None)
    else:
        compositor.reload()


def _publish_lock_login(home, state, colors, wallpaper):
    return _call("classic-state", "_publish_lock_login", home, state, colors, wallpaper)


def __getattr__(name):
    if name == "DEMO_PREFERENCES":
        return tuple(_call("classic-state", name))
    raise AttributeError(name)


def publish_compositor_palette(home, colors):
    return _call("classic-state", "publish_compositor_palette", home, colors)
