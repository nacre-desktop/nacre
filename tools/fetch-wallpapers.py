#!/usr/bin/env python3
"""Fetch the curated trial collection; image files stay outside the public OS repo."""

import json
import urllib.request
from pathlib import Path

collection = Path.home() / "Pictures/Wallpapers/Nacre"
collection.mkdir(parents=True, exist_ok=True)
for image in json.loads(
    (
        Path(__file__).resolve().parents[1]
        / "nacre/shell-tools/wallpaper-collection.json"
    ).read_text()
):
    path = collection / image["filename"]
    if not path.exists():
        request = urllib.request.Request(image["url"], headers={"User-Agent": "Nacre"})
        payload = urllib.request.urlopen(request, timeout=60).read()
        temporary = path.with_suffix(path.suffix + ".download")
        temporary.write_bytes(payload)
        temporary.replace(path)
print("Collection available in", collection)
