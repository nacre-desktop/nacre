#!/usr/bin/env python3
"""Track-keyed private lyrics/Orient artwork cache; never publishes desktop colors."""

import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import sys
import time
import urllib.parse
import urllib.request

from PIL import Image

HOME = Path.home()
CACHE = HOME / ".cache/nacre/dashboard-media"


def get(url, maximum):
    parsed = urllib.parse.urlsplit(url)
    if (
        parsed.scheme != "https"
        or not parsed.hostname
        or parsed.username
        or parsed.password
    ):
        raise ValueError("Artwork must use HTTPS")
    with urllib.request.urlopen(
        urllib.request.Request(
            url, headers={"User-Agent": "Nacre/1.0 (desktop media)"}
        ),
        timeout=6,
    ) as response:
        if urllib.parse.urlsplit(response.geturl()).scheme != "https":
            raise ValueError("HTTPS redirect required")
        data = response.read(maximum + 1)
        if len(data) > maximum:
            raise ValueError("Response too large")
        return data


def lyrics(text):
    lines = []
    for line in str(text or "").splitlines()[:3000]:
        matches = list(re.finditer(r"\[(\d{1,3}):(\d{2}(?:\.\d{1,3})?)\]", line))
        content = re.sub(r"\[[^\]]*\]", "", line).strip()[:500]
        for match in matches:
            lines.append(dict(time=int(match[1]) * 60 + float(match[2]), text=content))
    return sorted(lines, key=lambda item: item["time"])[:2000]


def track_data(meta):
    identity = hashlib.sha256(json.dumps(meta, sort_keys=True).encode()).hexdigest()
    CACHE.mkdir(parents=True, exist_ok=True, mode=0o700)
    path = CACHE / (identity + ".json")
    if path.exists():
        cached = json.loads(path.read_text())
        if time.time() - path.stat().st_mtime < cached.get("cacheSeconds", 1800):
            return cached
    result = dict(
        key=identity,
        lines=[],
        plain="",
        palette={},
        lyricsMessage="No synced lyrics found",
    )
    if meta.get("lyrics") and meta.get("title") and meta.get("artist"):
        try:
            query = urllib.parse.urlencode(
                dict(
                    track_name=meta["title"],
                    artist_name=meta["artist"],
                    album_name=meta.get("album", ""),
                    duration=round(meta.get("duration", 0)),
                )
            )
            data = json.loads(get("https://lrclib.net/api/get?" + query, 300000))
            result.update(
                lines=lyrics(data.get("syncedLyrics")),
                plain=str(data.get("plainLyrics") or "")[:60000],
                lyricsMessage="Instrumental"
                if data.get("instrumental")
                else "No synced lyrics found",
            )
        except (OSError, ValueError, KeyError, TypeError):
            result["lyricsMessage"] = "Lyrics unavailable for this track"
    if meta.get("palette") and meta.get("art"):
        try:
            from orient.engine import from_image

            art = meta["art"]
            parsed = urllib.parse.urlsplit(art)
            if parsed.scheme == "file":
                original = Path(urllib.parse.unquote(parsed.path))
                if original.stat().st_size > 8_000_000:
                    raise ValueError("Artwork too large")
            else:
                original = CACHE / (identity + ".image")
                original.write_bytes(get(art, 4_000_000))
                original.chmod(0o600)
            with Image.open(original) as image:
                if image.width * image.height > 16_000_000:
                    raise ValueError("Artwork dimensions too large")
                image.thumbnail((512, 512))
                poster = CACHE / (identity + ".png")
                image.convert("RGB").save(poster)
                poster.chmod(0o600)
            colors = from_image(poster, mode=meta.get("mode", "dark"))["colours"]
            result["palette"] = {
                key: "#" + colors[key].lstrip("#")
                for key in ("surface", "onSurface", "primary", "secondary", "tertiary")
            }
        except (OSError, ValueError, KeyError, TypeError, Image.DecompressionBombError):
            result["palette"] = {}
    result["cacheSeconds"] = (
        14 * 86400
        if result["lines"]
        or result["plain"]
        or result["lyricsMessage"] == "Instrumental"
        or not meta.get("lyrics")
        else 1800
    )
    path.write_text(json.dumps(result))
    path.chmod(0o600)
    # Bound cache growth; thumbnails/artwork share each entry identity.
    entries = sorted(
        CACHE.glob("*.json"), key=lambda item: item.stat().st_mtime, reverse=True
    )
    for old in entries[80:]:
        for item in CACHE.glob(old.stem + ".*"):
            item.unlink(missing_ok=True)
    return result


def spectrum():
    if not shutil.which("cava"):
        raise SystemExit(0)
    CACHE.mkdir(parents=True, exist_ok=True, mode=0o700)
    path = CACHE / "cava.conf"
    path.write_text(
        "[general]\nframerate=20\nbars=32\nautosens=1\n[input]\nmethod=pipewire\nsource=auto\n[output]\nmethod=raw\nraw_target=/dev/stdout\ndata_format=ascii\nascii_max_range=1000\nchannels=mono\n"
    )
    os.execvp("cava", ["cava", "-p", str(path)])


if __name__ == "__main__":
    if sys.argv[1] == "spectrum":
        spectrum()
    else:
        try:
            print(json.dumps(track_data(json.loads(sys.argv[1]))))
        except (OSError, ValueError, KeyError, TypeError):
            print(
                json.dumps(
                    dict(
                        lines=[], palette={}, lyricsMessage="Media details unavailable"
                    )
                )
            )
