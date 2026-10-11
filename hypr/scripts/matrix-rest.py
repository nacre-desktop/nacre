#!/usr/bin/env python3
"""Nacre's terminal-only quiet rain view; canonical colors and reversible modes."""

from collections import deque
from dataclasses import dataclass
import os
from pathlib import Path
import random
import re
import select
import shutil
import signal
import sys
import termios
import time
import tty

GLYPHS = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz<>[]{}+-/\\|#@"
PERIOD = 0.1
ENTER = "\x1b[?1049h\x1b[?25l\x1b[?1000h\x1b[?1006h"
LEAVE = "\x1b[0m\x1b[?1006l\x1b[?1000l\x1b[?25h\x1b[?1049l"


def read_rgb(role, fallback):
    try:
        value = (Path.home() / ".config/nacre/colors" / role).read_text().strip()
        if not re.fullmatch(r"#?[0-9a-fA-F]{6}", value):
            return fallback
        value = value.removeprefix("#")
        return tuple(int(value[offset : offset + 2], 16) for offset in (0, 2, 4))
    except OSError:
        return fallback


def blend(first, second, weight):
    return tuple(
        round(a * (1 - weight) + b * weight) for a, b in zip(first, second, strict=True)
    )


def dim(color, multiplier):
    return tuple(round(channel * multiplier) for channel in color)


def load_palette():
    accent = read_rgb("primary", (110, 255, 160))
    support = read_rgb("secondary", (176, 255, 210))
    ink = read_rgb("onsurface", (232, 243, 236))
    body = read_rgb("surface", (8, 17, 12))
    backdrop = dim(blend(body, accent, 0.06), 0.22)
    trail = blend(dim(accent, 0.72), ink, 0.12)
    return {
        "background": backdrop,
        "primary": trail,
        "secondary": blend(dim(support, 0.58), backdrop, 0.18),
        "highlight": blend(ink, trail, 0.22),
    }


def color_escape(color, background=False):
    return (
        "\x1b["
        + ("48" if background else "38")
        + ";2;"
        + ";".join(map(str, color))
        + "m"
    )


@dataclass
class Column:
    x: int
    head: float
    speed: float
    length: int
    cells: deque


class Rain:
    def __init__(self, rng=None):
        self.rng = random.Random() if rng is None else rng
        self.geometry = (0, 0)
        self.columns = []

    def resize(self, width, height):
        geometry = (max(1, width), max(1, height))
        if geometry == self.geometry:
            return
        self.geometry = geometry
        width, height = geometry
        spacing = 2 if width >= 100 else 1
        self.columns = []
        for x in range(0, width, spacing):
            length = self.rng.randint(max(6, height // 7), max(12, height // 3))
            cells = deque(
                (self.rng.choice(GLYPHS) for _ in range(length)), maxlen=length
            )
            self.columns.append(
                Column(
                    x,
                    self.rng.uniform(-height, height),
                    self.rng.uniform(0.65, 1.8),
                    length,
                    cells,
                )
            )

    def frame(self, colors):
        width, height = self.geometry
        parts = [color_escape(colors["background"], True), "\x1b[2J"]
        for column in self.columns:
            previous = int(column.head)
            column.head += column.speed
            for _ in range(max(0, int(column.head) - previous)):
                column.cells.appendleft(self.rng.choice(GLYPHS))
            if column.head - column.length >= height:
                column.head = self.rng.uniform(-height, 0)
                column.speed = self.rng.uniform(0.65, 1.8)
            for offset, glyph in enumerate(column.cells):
                row = int(column.head) - offset
                if not 0 <= row < height or not 0 <= column.x < width:
                    continue
                color = (
                    colors["highlight"]
                    if offset == 0
                    else colors["primary"]
                    if offset < 3
                    else blend(
                        colors["background"],
                        colors["secondary"],
                        0.25 + 0.55 * (1 - offset / column.length),
                    )
                )
                parts.extend(
                    (f"\x1b[{row + 1};{column.x + 1}H", color_escape(color), glyph)
                )
        return "".join(parts)


def run():
    if not sys.stdin.isatty() or not sys.stdout.isatty():
        raise RuntimeError("The rest view requires a terminal")
    original = termios.tcgetattr(sys.stdin.fileno())
    stopped = False

    def stop(_signum, _frame):
        nonlocal stopped
        stopped = True

    handlers = {
        number: signal.signal(number, stop)
        for number in (signal.SIGINT, signal.SIGTERM)
    }
    rain = Rain()
    palette = load_palette()
    started = time.monotonic()
    try:
        tty.setcbreak(sys.stdin.fileno())
        sys.stdout.write(ENTER)
        sys.stdout.flush()
        while not stopped:
            tick = time.monotonic()
            ready, _, _ = select.select([sys.stdin], [], [], 0)
            if ready:
                data = os.read(sys.stdin.fileno(), 128)
                if tick - started >= 0.35 and data:
                    break
            size = shutil.get_terminal_size((120, 40))
            rain.resize(size.columns, size.lines)
            sys.stdout.write(rain.frame(palette))
            sys.stdout.flush()
            time.sleep(max(0, PERIOD - (time.monotonic() - tick)))
    finally:
        try:
            sys.stdout.write(LEAVE)
            sys.stdout.flush()
        finally:
            termios.tcsetattr(sys.stdin.fileno(), termios.TCSADRAIN, original)
            for number, handler in handlers.items():
                signal.signal(number, handler)


if __name__ == "__main__":
    run()
