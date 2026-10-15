"""CLI boundary for current scripts and a replacement helper executable."""

import os
import sys
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1]


def command(name, *arguments):
    binary = os.environ.get("NACRE_TEST_HELPER_BINARY")
    if binary:
        return [binary, name, *arguments]
    if name == "wallpaper-watch":
        from nacre_native_config import _binary

        return [str(_binary()), name, *arguments]
    return [sys.executable, str(TOOLS / (name + ".py")), *arguments]
