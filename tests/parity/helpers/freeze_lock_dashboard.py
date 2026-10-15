#!/usr/bin/env python3
"""Capture unchanged Python lock artwork once using explicit public fixture inputs."""

import base64
import gzip
import hashlib
import importlib.util
import io
import json
import os
import sys
import tempfile
from datetime import datetime
from pathlib import Path
from unittest.mock import patch
from PIL import Image, ImageFont, __version__

root = Path(__file__).resolve().parents[3]
tools = root / "nacre/shell-tools"
folder = root / "crates/nacre-helper/tests/fixtures"
dest = folder / "lock-dashboard-reference.json.gz"
assert not dest.exists()
source = tools / "lock-dashboard.py"
assert (
    hashlib.sha256(source.read_bytes()).hexdigest()
    == "dfa7de6d0656a055edc3c5a288e20ff7699045938e48437394f531e43a6fccb1"
)
sys.path.insert(0, str(tools))
spec = importlib.util.spec_from_file_location("lock_reference", source)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class Clock(datetime):
    @classmethod
    def now(cls, tz=None):
        return cls(2026, 7, 14, 14, 30, tzinfo=tz)


fonts = folder / "lock-fonts"


def fixture_font(size, family="IBM Plex Sans", style="Regular"):
    return ImageFont.truetype(
        str(
            fonts
            / (
                "FixtureSymbols.ttf"
                if family == "Material Symbols Rounded"
                else "FixtureSans.ttf"
            )
        ),
        round(size * 2),
    )


colors = json.loads((tools / "reference-style.json").read_text())["colours"]
inputs = {}
cases = []
text_cases = []
for font in ("FixtureSans.ttf", "FixtureSymbols.ttf"):
    for size in (
        (22, 28, 32, 40, 60, 76) if font == "FixtureSans.ttf" else (38, 50, 62, 76, 84)
    ):
        face = ImageFont.truetype(str(fonts / font), size)
        for text in (
            (
                "Nacre",
                "Good afternoon, Fixture",
                "22°C · Cached forecast",
                "Message previews hidden",
                "Longer text …",
                "Hello مرحبا",
            )
            if font == "FixtureSans.ttf"
            else tuple(chr(c) for c in (0xE164, 0xE81A, 0xE322, 0xE7F4))
        ):
            mask, offset = face.getmask2(text)
            text_cases.append(
                dict(
                    font=font,
                    size=size,
                    text=text,
                    advance=face.getlength(text),
                    offset=list(offset),
                    mask_size=list(mask.size),
                    mask=base64.b64encode(bytes(mask)).decode(),
                )
            )
for name in ("empty", "populated-safe", "populated-private", "wide-wallpaper"):
    with tempfile.TemporaryDirectory(prefix="nacre-lock-panel-reference-") as directory:
        home = Path(directory)
        data = {}
        wallpaper = ""
        monitor = None
        if name != "empty":
            data = dict(
                weather=dict(
                    temperature="22°C",
                    description="Clear sky",
                    location="Fixture city",
                    range="18° / 24°",
                    icon="sunny",
                ),
                media=dict(
                    title="Fixture song", artist="Fixture artist", album="Fixture album"
                ),
                system=dict(cpu=0.37, memory=0.64, storage=0.22, temperature=48),
                batteryLabel="80% · 4h remaining",
                notifications=[
                    dict(
                        app="Fixture app",
                        summary="First <b>message</b>",
                        body="A public fixture body",
                        time="2026-07-14T14:18:00",
                    ),
                    dict(
                        app="Fixture app",
                        summary="Second message",
                        body="Another public fixture",
                    ),
                    dict(
                        app="Other app", summary="Update complete", body="Public test"
                    ),
                ],
                preferences=dict(lockNotificationContents=name == "populated-private"),
            )
        if name == "wide-wallpaper":
            image = Image.new("RGB", (37, 23))
            image.putdata(
                [
                    (
                        (x * 19 + y * 3) % 256,
                        (x * 7 + y * 29) % 256,
                        (x * 11 + y * 13) % 256,
                    )
                    for y in range(23)
                    for x in range(37)
                ]
            )
            buffer = io.BytesIO()
            image.save(buffer, format="PNG")
            raw = buffer.getvalue()
            inputs["wallpaper"] = base64.b64encode(raw).decode()
            file = home / "wallpaper.png"
            file.write_bytes(raw)
            wallpaper = str(file)
            monitor = dict(name="fixture-wide", width=3440, height=1440, transform=0)
        with (
            patch.object(module, "font", side_effect=fixture_font),
            patch.object(
                module,
                "system_info",
                return_value=dict(os="Linux", user="Fixture", uptime="1h 00m"),
            ),
            patch.object(module, "datetime", Clock),
            patch.dict(os.environ, dict(NACRE_LANGUAGE="en")),
        ):
            picture = module.render(
                data,
                colors,
                wallpaper,
                str(home / "missing-artwork.png"),
                home,
                monitor,
            )
        buffer = io.BytesIO()
        picture.save(buffer, format="PNG", compress_level=2)
        cases.append(
            dict(
                name=name,
                data=data,
                colors=colors,
                wallpaper="wallpaper" if wallpaper else None,
                monitor=monitor,
                size=list(picture.size),
                png=base64.b64encode(buffer.getvalue()).decode(),
            )
        )
value = dict(
    source_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),
    pillow=__version__,
    clock="2026-07-14T14:30:00",
    system_info=dict(os="Linux", user="Fixture", uptime="1h 00m"),
    font_provenance=json.loads((fonts / "provenance.json").read_text()),
    normalizations="Disposable home/path only; all PNG bytes and text mask/advance/offset values exact. System/account/time and pinned subset fonts are explicit fixture inputs. No personal data.",
    inputs=inputs,
    text_cases=text_cases,
    cases=cases,
)
dest.write_bytes(gzip.compress((json.dumps(value, indent=2) + "\n").encode(), mtime=0))
print(
    "Frozen",
    len(cases),
    "cold panels and",
    len(text_cases),
    "text masks/advances; compressed bytes",
    dest.stat().st_size,
)
