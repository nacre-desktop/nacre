"""Developer-only image/Path adaptation retaining the original branding assertions."""

import base64
import io
from pathlib import Path
from PIL import Image
from nacre_native_palette import _call

ROOT = Path(__file__).resolve().parents[3]
__file__ = str(ROOT / "tools/generate_branding.py")
MASTER = ROOT / "nacre/shell/branding/nacre-master.svg"
GEOMETRY = MASTER
VIEWBOX = "2.8 3 58 58"
APPLICATIONS = ("ai", "brain", "settings", "colors", "notifications")


def templates():
    return _call("branding", "templates")


def role_colors(colors):
    return _call("branding", "role_colors", colors)


def asset_names():
    return _call("branding", "asset_names")


def paint_svg(text, roles):
    return _call("branding", "paint_svg", text, roles)


def svg(template=True, colors=None, variant="full"):
    return _call("branding", "svg", template, colors, variant)


def luminance(value):
    return _call("branding", "luminance", value)


def raster(text, size=512):
    raw = base64.b64decode(_call("branding", "raster", text, size))
    with Image.open(io.BytesIO(raw)) as image:
        return image.copy()


def cached_png(text, home, size=512):
    return base64.b64decode(_call("branding", "cached_png", text, home, size))


def prepare(colors, home):
    return _call("branding", "prepare", colors, home)


def publish(colors, home=None):
    return _call("branding", "publish", colors, home)


def refresh_terminal_menus(home, proc):
    return _call("branding", "refresh_terminal_menus", home, proc)
