"""Developer-only mapping of historical patched media paths to native methods."""

from functools import partial
import time
from pathlib import Path
from native_lock_dashboard import invoke

METHODS = (
    "settings",
    "media_state",
    "palette_presets",
    "preference",
    "previews",
    "describe",
    "catalog",
    "palette_marker",
)


def call(media, method, *args, **kwargs):
    if method == "palette_marker":
        names = ("poster", "flavour", "harmony", "source", "mode", "personality")
        args = tuple(args) + tuple(kwargs.get(name) for name in names[len(args) :])
    result = invoke(
        "media",
        method=method,
        args=[str(v) if hasattr(v, "__fspath__") else v for v in args],
        home=str(media.HOME),
        library=str(media.LIBRARY),
        cache=str(media.CACHE),
        state=str(media.STATE),
        prefs=str(media.PREFS),
        now_ms=int(time.time() * 1000),
    )

    return Path(result) if method == "palette_marker" else result


def install(media):
    for method in METHODS:
        setattr(media, method, partial(call, media, method))
    return media
