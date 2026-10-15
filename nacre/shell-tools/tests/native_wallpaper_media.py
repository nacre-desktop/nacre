"""Developer-only historical module surface; behavior executes in Rust."""

import os
from pathlib import Path
import subprocess
import sys
import time as time
from native_media_subset import install

HOME = Path.home()
LIBRARY = HOME / "Pictures/Wallpapers"
CACHE = HOME / ".cache/nacre/wallpaper-media"
STATE = HOME / ".local/state/nacre/wallpaper"
PREFS = HOME / ".config/nacre/wallpaper-picker.json"
ROOT = Path(__file__).resolve().parents[3]
__file__ = str(ROOT / "nacre/shell-tools/wallpaper-media.py")


def demo_module(name):
    if name == "classic-state":
        import native_classic_state

        return native_classic_state
    raise AssertionError("Use native demo dependencies")


def serve(input_stream, output_stream):
    binary = os.environ.get(
        "NACRE_HELPER_BINARY", str(ROOT / "target/debug/nacre-helper")
    )
    child = subprocess.Popen(
        [binary, "wallpaper-media", "serve"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        text=True,
    )
    stdout, _ = child.communicate(input=input_stream.read(), timeout=120)
    output_stream.write(stdout)
    return child.returncode


install(sys.modules[__name__])
