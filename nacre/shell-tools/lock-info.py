#!/usr/bin/env python3
"""Read-only lock widgets and an allowlist of media actions. No authentication data."""

import argparse
from nacre_i18n import tr
import fcntl
import html
import json
import os
import subprocess
import time
import urllib.request
import urllib.parse
import io
from PIL import Image, ImageFont
from functools import lru_cache
import math
from pathlib import Path
import hashlib

HOME = Path.home()
CACHE = HOME / ".cache/nacre/lock-widgets.json"


def ipc(target, action):
    result = subprocess.run(
        [
            str(HOME / ".local/share/nacre/shell/bin/qs"),
            "-c",
            "nacre",
            "ipc",
            "call",
            target,
            action,
        ],
        capture_output=True,
        text=True,
        timeout=2,
    )
    if result.returncode:
        raise RuntimeError(
            tr("helper.lock-info.desktop-data-unavailable", "Desktop data unavailable")
        )
    return result.stdout


def snapshot():
    CACHE.parent.mkdir(parents=True, exist_ok=True)
    with CACHE.with_suffix(".lock").open("w") as guard:
        os.chmod(guard.name, 0o600)
        fcntl.flock(guard, fcntl.LOCK_EX)
        try:
            cached = json.loads(CACHE.read_text())
            age = time.time() - cached.get("time", 0)
            if 0 <= age < 2:
                return cached["data"]
        except (OSError, ValueError, TypeError, KeyError):
            pass
        try:
            data = json.loads(ipc("lockWidgets", "state"))
        except (
            OSError,
            ValueError,
            KeyError,
            TypeError,
            subprocess.SubprocessError,
            Image.DecompressionBombError,
        ):
            return {}  # Never display indefinitely stale notification contents.
        temp = CACHE.with_suffix(".tmp")
        temp.write_text(json.dumps(dict(time=time.time(), data=data)))
        temp.chmod(0o600)
        temp.replace(CACHE)
        return data


@lru_cache(maxsize=4)
def text_font(size):
    path = subprocess.check_output(
        ["fc-match", "--format=%{file}", "IBM Plex Sans:style=Bold"],
        text=True,
        timeout=2,
    )
    return ImageFont.truetype(path, math.ceil(size * 96 / 72))


def plain(value, width=288, lines=2, font=14):
    text = " ".join(str(value or "").split())[:2048]
    face = text_font(font)
    wrapped, line = [], ""
    for character in text:
        candidate = line + character
        if face.getlength(candidate) > width and line:
            space = line.rfind(" ")
            if space > 0:
                wrapped.append(line[:space].rstrip())
                line = line[space + 1 :] + character
            else:
                wrapped.append(line.rstrip())
                line = character.lstrip()
            if len(wrapped) > lines:
                break
        else:
            line = candidate
    if line:
        wrapped.append(line.rstrip())
    if len(wrapped) > lines:
        wrapped = wrapped[:lines]
        while wrapped[-1] and face.getlength(wrapped[-1] + "…") > width:
            wrapped[-1] = wrapped[-1][:-1]
        wrapped[-1] += "…"
    return html.escape("\n".join(wrapped))


def label(kind, data, settings):
    if kind == "weather":
        if not settings.get("lockWeather", True):
            return ""
        w = data.get("weather", {})
        desc = w.get("description")
        return tr(
            "helper.lock-info.b-value0-b-value1-value2",
            "<b>{value0}</b>\n\n{value1}\n{value2}",
            {
                "value0": plain(w.get("location") or "Weather", lines=1, font=16),
                "value1": plain(
                    w.get("temperature", "") if desc else "", lines=1, font=16
                ),
                "value2": plain(
                    desc or w.get("error") or "Weather unavailable", font=16
                ),
            },
        )
    if kind == "media":
        if not settings.get("lockMedia", True):
            return ""
        m = data.get("media")
        if not m:
            return tr(
                "helper.lock-info.b-media-b-nothing-playing",
                "<b>Media</b>\n\nNothing playing",
            )
        return tr(
            "helper.lock-info.b-value0-b-value1-value2",
            "<b>{value0}</b>\n{value1}\n\n{value2}",
            {
                "value0": plain(m.get("title") or "Untitled track"),
                "value1": plain(m.get("artist")),
                "value2": "Playing" if m.get("playing") else "Paused",
            },
        )
    if kind == "notifications":
        if not settings.get("lockNotifications", True):
            return ""
        rows = data.get("notifications", [])
        count = data.get("count", len(rows))
        output = ["<b>Notifications · " + str(count) + "</b>"]
        if not rows:
            return tr(
                "helper.lock-info.value0-you-are-all-caught-up",
                "{value0}\n\nYou are all caught up",
                {"value0": output[0]},
            )
        if settings.get("lockNotificationContents", False):
            for n in rows[:3]:
                output += [
                    "",
                    "<b>" + plain(n.get("app") or "Notification", lines=1) + "</b>",
                    plain(n.get("summary")),
                    plain(n.get("body"), lines=1),
                ]
        else:
            counts = {}
            for n in rows:
                counts[n.get("app") or "Notification"] = (
                    counts.get(n.get("app") or "Notification", 0) + 1
                )
            output += ["", "Message previews hidden", ""] + [
                plain(app, lines=1) for app in list(counts)[:5]
            ]
        return "\n".join(output)
    if kind == "status":
        batteries = []
        for path in Path("/sys/class/power_supply").glob("*/capacity"):
            try:
                batteries.append(path.read_text().strip() + "% battery")
            except OSError:
                pass
        return html.escape(" · ".join(batteries) or "Nacre")
    if kind == "play-icon":
        return "pause" if (data.get("media") or {}).get("playing") else "play_arrow"
    return ""


def artwork(data):
    fallback = HOME / ".local/share/nacre/branding/nacre-lock.png"
    url = (data.get("media") or {}).get("art", "")
    fingerprint = url
    if url.startswith("file:"):
        try:
            stat = Path(urllib.parse.unquote(urllib.parse.urlparse(url).path)).stat()
            fingerprint += f":{stat.st_mtime_ns}:{stat.st_size}"
        except OSError:
            pass
    output = CACHE.with_name(
        "lock-art-" + hashlib.sha256(fingerprint.encode()).hexdigest()[:20] + ".png"
    )
    if output.exists():
        return str(output)
    metadata = CACHE.with_name("lock-art-source.json")
    try:
        saved = json.loads(metadata.read_text())
        if saved.get("fingerprint") == fingerprint and (
            (saved.get("ok") and output.exists())
            or time.time() - saved.get("checked", 0) < 120
        ):
            return str(output if saved.get("ok") and output.exists() else fallback)
    except (OSError, ValueError):
        pass
    ok = False
    try:
        parsed = urllib.parse.urlparse(url)
        if parsed.scheme == "file":
            with Path(urllib.parse.unquote(parsed.path)).open("rb") as source:
                raw = source.read(4000001)
        elif parsed.scheme == "https":
            with urllib.request.urlopen(url, timeout=4) as response:
                if urllib.parse.urlparse(response.url).scheme != "https":
                    raise ValueError(
                        tr(
                            "helper.lock-info.unsupported-artwork-redirect",
                            "Unsupported artwork redirect",
                        )
                    )
                raw = response.read(4000001)
        else:
            raise ValueError(tr("helper.lock-info.no-artwork", "No artwork"))
        if len(raw) > 4000000:
            raise ValueError(
                tr("helper.lock-info.artwork-too-large", "Artwork too large")
            )
        with Image.open(io.BytesIO(raw)) as picture:
            if picture.width * picture.height > 20000000:
                raise ValueError(
                    tr(
                        "helper.lock-info.artwork-dimensions-too-large",
                        "Artwork dimensions too large",
                    )
                )
            picture.thumbnail((512, 512))
            temporary = output.with_suffix(".next")
            picture.convert("RGBA").save(temporary, format="PNG")
            temporary.replace(output)
            output.chmod(0o600)
            ok = True
    except (
        OSError,
        ValueError,
        KeyError,
        TypeError,
        subprocess.SubprocessError,
        Image.DecompressionBombError,
    ):
        pass
    metadata.write_text(
        json.dumps(dict(fingerprint=fingerprint, checked=time.time(), ok=ok))
    )
    metadata.chmod(0o600)
    return str(output if ok else fallback)


def main():
    p = argparse.ArgumentParser()
    p.add_argument(
        "kind",
        choices=[
            "weather",
            "media",
            "notifications",
            "status",
            "art",
            "play-icon",
            "previous",
            "next",
            "toggle",
        ],
    )
    a = p.parse_args()
    try:
        settings = json.loads((HOME / ".config/nacre/desktop.json").read_text())
    except (OSError, ValueError):
        settings = {}
    if a.kind in ("previous", "next", "toggle"):
        if settings.get("lockMedia", True):
            ipc("mpris", "playPause" if a.kind == "toggle" else a.kind)
        return
    data = snapshot()
    if a.kind == "art":
        print(artwork(data) if settings.get("lockMedia", True) else "")
    else:
        print(label(a.kind, data, settings))


if __name__ == "__main__":
    main()
