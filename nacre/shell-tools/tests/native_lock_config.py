"""Developer-only mapping of the historical lock configuration module API."""

from pathlib import Path
from nacre_native_palette import _call


def block(widget, monitor="", **options):
    return _call("lock-config", "block", widget, monitor, options)


def panel_key(monitor):
    return _call("lock-config", "panel_key", monitor)


def panel_scale(monitor):
    return _call("lock-config", "panel_scale", monitor)


def render(
    colors, wallpaper, preferences, helper, monitors=None, ready=None, artwork=None
):
    return _call(
        "lock-config",
        "render",
        colors,
        wallpaper,
        preferences,
        helper,
        monitors,
        ready,
        artwork,
    )


def prepare_config(colors, wallpaper, preferences, helper, home=None):
    return _call(
        "lock-config", "prepare_config", colors, wallpaper, preferences, helper, home
    )


def publish(home=None):
    return _call("lock-config", "publish", Path.home() if home is None else home)
