"""Developer-only globals/clock mapping; lock behavior executes in Rust."""

import json
import math
from functools import lru_cache
import os
from pathlib import Path
import subprocess
import tempfile
import time as time
from unittest.mock import Mock
from PIL import Image as Image, ImageFont

ROOT = Path(__file__).resolve().parents[3]
HOME = Path.home()
CACHE = HOME / ".cache/nacre/lock-widgets.json"


def ipc(target, action):
    raise AssertionError("Use a private shell fixture for native lock IPC")


def invoke(operation, **values):
    request = dict(
        operation=operation, home=str(HOME), cache=str(CACHE), now=time.time()
    )
    request.update(values)
    binary = os.environ.get(
        "NACRE_LOCK_PARITY", str(ROOT / "target/debug/examples/lock-parity")
    )
    child = subprocess.Popen(
        [binary],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    try:
        stdout, stderr = child.communicate(json.dumps(request), timeout=120)
    except BaseException:
        child.kill()
        child.communicate()
        raise
    reply = json.loads(stdout)
    if child.returncode:
        raise ValueError(reply.get("error", stderr[-500:]))
    return reply["result"]


def snapshot():
    with tempfile.TemporaryDirectory(prefix="nacre-lock-ipc-observer-") as folder:
        log = Path(folder) / "requests"
        request = {}
        if isinstance(ipc, Mock):
            request = dict(ipc_output=ipc.return_value, ipc_log=str(log))
        result = invoke("snapshot", **request)
        if log.exists():
            ipc("lockWidgets", "state")
        return result


def artwork(data):
    return invoke("artwork", data=data)


def label(kind, data, settings):
    return invoke("label", kind=kind, data=data, settings=settings)


def plain(value, width=288, lines=2, font=14):
    return invoke("plain", value=value, width=width, lines=lines, font_size=font)


@lru_cache(maxsize=4)
def text_font(size):
    # Independent Pillow metric oracle for the retained width assertions only.
    path = subprocess.check_output(
        ["fc-match", "--format=%{file}", "IBM Plex Sans:style=Bold"],
        text=True,
        timeout=2,
    )
    return ImageFont.truetype(path, math.ceil(size * 96 / 72))
