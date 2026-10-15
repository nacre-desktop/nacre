"""Developer-only globals adapter; all rendering/publication executes in Rust."""

import base64
from datetime import datetime
import io
import json
import os
from pathlib import Path
import subprocess
from PIL import Image

ROOT = Path(__file__).resolve().parents[3]
WIDTH, HEIGHT, RESOLUTION = 1440, 810, 2


def invoke(operation, **request):
    request["operation"] = operation
    request.setdefault("now", datetime.now().astimezone().isoformat())
    binary = os.environ.get(
        "NACRE_PALETTE_LEGACY_PARITY",
        str(ROOT / "target/debug/examples/palette-legacy-parity"),
    )
    process = subprocess.Popen(
        [binary], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True
    )
    try:
        stdout, _ = process.communicate(input=json.dumps(request), timeout=60)
    except BaseException:
        process.kill()
        process.communicate()
        raise
    reply = json.loads(stdout)
    if process.returncode:
        raise ValueError(reply["error"])
    return reply["result"]


def system_info(home):
    return invoke("system_info", home=str(home))


def layout(monitor):
    return invoke("layout", monitor=monitor)


def notification_icon(name, app, home):
    value = invoke("notification_icon", name=name, app=app, home=str(home))
    return Path(value) if value else None


def render(data, colors, wallpaper, artwork, home, monitor=None):
    value = invoke(
        "lock_render",
        data=data,
        colors=colors,
        wallpaper=str(wallpaper),
        artwork=str(artwork) if artwork is not None else None,
        home=str(home),
        monitor=monitor,
        system_info=system_info(home),
    )
    with Image.open(io.BytesIO(base64.b64decode(value))) as image:
        return image.copy()


def publish(data, colors, wallpaper, artwork, home, monitors):
    return invoke(
        "lock_publish",
        data=data,
        colors=colors,
        wallpaper=str(wallpaper),
        artwork=str(artwork) if artwork is not None else None,
        home=str(home),
        monitors=monitors,
        system_info=system_info(home),
    )
