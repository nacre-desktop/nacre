"""Developer-only old globals facade; all preview/state behavior executes in Rust."""

import os
from pathlib import Path
import subprocess
from types import SimpleNamespace
from nacre_native_palette import _call

ROOT = Path(__file__).resolve().parents[3]
HOME = Path.home()
CACHE = HOME / ".cache/nacre/colors"
STORE = HOME / ".config/nacre/colors.json"
EXPORTS = HOME / "Documents/Nacre Colors"
media = SimpleNamespace(
    HOME=HOME,
    LIBRARY=HOME / "Pictures/Wallpapers",
    CACHE=HOME / ".cache/nacre/wallpaper-media",
    STATE=HOME / ".local/state/nacre/wallpaper",
    PREFS=HOME / ".config/nacre/wallpaper-picker.json",
)


media.settings = lambda: _call("wallpaper-media", "settings")


def catalog():
    return _call("colors", "catalog")


def from_image(*args, **kwargs):
    raise AssertionError("Native previews must compute in-process")


def stage(request):
    return _call("colors", "stage", request)


def record(identity):
    return _call("colors", "record", identity)


def stored():
    return _call("colors", "stored")


def favorite(identity, name=""):
    return _call("colors", "favorite", identity, name)


def apply(identity, live=True):
    return _call("colors", "apply", identity, live=live)


def serve(input_stream, output_stream):
    binary = os.environ.get(
        "NACRE_HELPER_BINARY", str(ROOT / "target/debug/nacre-helper")
    )
    process = subprocess.Popen(
        [binary, "colors", "serve"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        text=True,
    )
    stdout, _ = process.communicate(input=input_stream.read(), timeout=120)
    output_stream.write(stdout)
    return process.returncode


def export(identity):
    return _call("colors", "export", identity)


def card(identity):
    return _call("colors", "card", identity)
