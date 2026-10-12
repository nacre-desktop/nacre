#!/usr/bin/env python3
"""Validate and cache the offline, public-domain Welcome wallpaper collection."""

import hashlib
from nacre_i18n import tr
import json
import os
from pathlib import Path
import shutil
import tempfile
from PIL import Image

ASSETS = Path(__file__).with_name("demo-wallpapers")


def collection():
    manifest = json.loads((ASSETS / "manifest.json").read_text())
    if manifest.get("version") != 1 or len(manifest.get("wallpapers", [])) != 4:
        raise ValueError(
            tr(
                "helper.demo-wallpapers.the-bundled-wallpaper-manifest-is-incomplete-reinstall-the-shell",
                "The bundled wallpaper manifest is incomplete. Reinstall the shell.",
            )
        )
    entries = manifest["wallpapers"]
    for item in entries:
        name = item["file"]
        if Path(name).name != name or Path(name).suffix not in (
            ".jpg",
            ".png",
            ".webp",
        ):
            raise ValueError(
                tr(
                    "helper.demo-wallpapers.invalid-bundled-wallpaper-filename",
                    "Invalid bundled wallpaper filename",
                )
            )
        source = ASSETS / name
        if hashlib.sha256(source.read_bytes()).hexdigest() != item["sha256"]:
            raise ValueError(
                "Bundled wallpaper checksum failed: " + name + ". Reinstall the shell."
            )
        with Image.open(source) as image:
            if (
                list(image.size) != item["size"]
                or image.width < 3840
                or image.height < 2000
            ):
                raise ValueError(
                    tr(
                        "helper.demo-wallpapers.bundled-wallpaper-resolution-does-not-match-its-manifest",
                        "Bundled wallpaper resolution does not match its manifest",
                    )
                )
            image.verify()
        if (
            item["license"] not in ("CC0",)
            or not item.get("artist")
            or not item.get("source")
        ):
            raise ValueError(
                tr(
                    "helper.demo-wallpapers.bundled-wallpaper-source-information-is-incomplete",
                    "Bundled wallpaper source information is incomplete",
                )
            )
    return entries


def ensure(home, fast=False):
    """Copy bundled bytes once by content hash, with no network or library writes.

    Immutable cached images keep an active wallpaper valid across software
    upgrades. A missing/corrupt cache copy is repaired from the public-domain bundle.
    The bundle is optimized for 4K displays; the thumbnail owner creates smaller previews.
    """
    manifest = (ASSETS / "manifest.json").read_bytes()
    identity = hashlib.sha256(manifest).hexdigest()[:16]
    folder = Path(home) / ".cache/nacre/demo-wallpapers" / identity
    folder.mkdir(parents=True, exist_ok=True)
    receipt = folder / "verified.json"
    entries = json.loads(manifest)["wallpapers"]

    def stamps():
        return {
            str(path): [
                path.stat().st_dev,
                path.stat().st_ino,
                path.stat().st_size,
                path.stat().st_mtime_ns,
                path.stat().st_ctime_ns,
            ]
            for item in entries
            for path in (ASSETS / item["file"], folder / item["file"])
        }

    if fast:
        try:
            if json.loads(receipt.read_text()) == stamps():
                return [dict(item, path=str(folder / item["file"])) for item in entries]
        except (OSError, ValueError):
            pass
    entries = collection()
    result = []
    for item in entries:
        source = ASSETS / item["file"]
        path = folder / item["file"]
        if (
            not path.is_file()
            or hashlib.sha256(path.read_bytes()).hexdigest() != item["sha256"]
        ):
            fd, name = tempfile.mkstemp(dir=folder, suffix=path.suffix)
            os.close(fd)
            try:
                shutil.copyfile(source, name)
                os.replace(name, path)
            finally:
                Path(name).unlink(missing_ok=True)
        result.append(dict(item, path=str(path)))
    fd, name = tempfile.mkstemp(dir=folder)
    try:
        with os.fdopen(fd, "w") as stream:
            json.dump(stamps(), stream)
        os.replace(name, receipt)
    finally:
        Path(name).unlink(missing_ok=True)
    return result


if __name__ == "__main__":
    print(json.dumps(ensure(Path.home())))
