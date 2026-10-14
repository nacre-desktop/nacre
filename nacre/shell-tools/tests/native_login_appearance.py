"""Developer-only mapping of old module globals to native publication policy."""

import json
import os
from pathlib import Path
import subprocess
from PIL import Image as Image

ROOT = Path(__file__).resolve().parents[3]
HOME = Path.home()
PUBLIC = Path("/var/lib/nacre/login/appearance")
DEFAULT_PUBLIC = PUBLIC
ROLES = {
    "surface": "surface",
    "raised": "surfaceContainer",
    "primary": "primary",
    "secondary": "secondary",
    "tertiary": "tertiary",
    "highlight": "primaryFixed",
    "text": "onSurface",
    "muted": "onSurfaceVariant",
    "error": "error",
}


def invoke(request):
    binary = os.environ.get("NACRE_LOGIN_PARITY") or str(
        ROOT / "target/debug/examples/login-appearance-parity"
    )
    result = subprocess.run(
        [binary, json.dumps(request)], capture_output=True, text=True, timeout=60
    )
    reply = json.loads(result.stdout)
    if result.returncode:
        kind = {
            "ValueError": ValueError,
            "KeyError": KeyError,
            "FileNotFoundError": FileNotFoundError,
        }.get(reply["type"], OSError)
        raise kind(reply["error"])
    return reply["result"]


def prepare(wallpaper, cache=None):
    result = invoke(
        {
            "operation": "prepare",
            "home": str(HOME),
            "wallpaper": str(wallpaper),
            "cache": str(cache) if cache is not None else None,
        }
    )
    return Path(result[0]), result[1]


def publish(colors, wallpaper, preview=None, public=DEFAULT_PUBLIC, *, home=None):
    return Path(
        invoke(
            {
                "operation": "publish",
                "home": str(home if home is not None else HOME),
                "colors": colors,
                "wallpaper": str(wallpaper),
                "preview": str(preview) if preview is not None else None,
                "public": str(public),
                "policy": {"base_home": str(HOME), "system_public": str(PUBLIC)},
            }
        )
    )
