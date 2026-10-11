#!/usr/bin/env python3
"""Publish only wallpaper/color data for the root-owned Nacre login theme."""

import configparser
import hashlib
import io
import json
import os
import tempfile
from pathlib import Path
from PIL import Image, ImageFilter, ImageOps

HOME = Path.home()
PREVIEW = HOME / ".local/state/nacre/shell/login-preview"
PUBLIC = Path("/var/lib/nacre/login/appearance")
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


def atomic(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(prefix="." + path.name, dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as f:
            f.write(data)
        os.chmod(tmp, 0o644)
        os.replace(tmp, path)
    finally:
        if os.path.exists(tmp):
            os.unlink(tmp)


def prepare(wallpaper, cache=None):
    image = Path(wallpaper).expanduser().resolve(strict=True)
    stat = image.stat()
    cache = Path(cache) if cache is not None else HOME / ".cache/nacre/login-images"
    cache.mkdir(parents=True, exist_ok=True, mode=0o700)
    stamp = f"{image}:{stat.st_mtime_ns}:{stat.st_size}:login-image-v1"
    target = cache / (hashlib.sha256(stamp.encode()).hexdigest() + ".png")
    if not target.exists():
        with Image.open(image) as original:
            original = ImageOps.exif_transpose(original).convert("RGB")
            original.thumbnail((2560, 1600))
            original = original.filter(ImageFilter.GaussianBlur(10))
            data = io.BytesIO()
            original.save(data, format="PNG")
        atomic(target, data.getvalue())
        target.chmod(0o600)
    return target, stamp


def publish(colors, wallpaper, preview=PREVIEW, public=PUBLIC):
    values = {key: "#" + colors[role].lstrip("#") for key, role in ROLES.items()}
    import re

    if any(not re.fullmatch("#[0-9a-fA-F]{6}", v) for v in values.values()):
        raise ValueError("Invalid login palette")
    preview.mkdir(parents=True, exist_ok=True, mode=0o700)
    preview.chmod(0o700)
    # Keep one versioned result per wallpaper, so returning to a previous image
    # copies its prepared blur instead of decoding/encoding it again.
    cached, stamp = prepare(wallpaper)
    if (
        not (preview / "background.png").exists()
        or not (preview / "image-stamp.txt").exists()
        or (preview / "image-stamp.txt").read_text() != stamp
    ):
        atomic(preview / "background.png", cached.read_bytes())
        atomic(preview / "image-stamp.txt", stamp.encode())
    destinations = [preview]
    if (
        public.is_dir()
        and public.stat().st_uid == os.getuid()
        and os.access(public, os.W_OK)
    ):
        destinations.append(public)
    for destination in destinations:
        if destination != preview:
            atomic(
                destination / "background.png",
                (preview / "background.png").read_bytes(),
            )
        conf = configparser.ConfigParser()
        conf["General"] = dict(values, background=str(destination / "background.png"))
        output = io.StringIO()
        conf.write(output)
        atomic(destination / "theme.conf", output.getvalue().encode())
    return preview


if __name__ == "__main__":
    state = HOME / ".local/state/nacre"
    colors = json.loads((state / "scheme.json").read_text())["colours"]
    wallpaper = (state / "wallpaper/last.txt").read_text().strip()
    publish(colors, wallpaper)
