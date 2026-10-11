#!/usr/bin/env python3
"""Debounced native directory events; no periodic library scans or new dependencies."""

import argparse
import ctypes
import json
import os
from pathlib import Path
import selectors
import struct
import time

LIBC = ctypes.CDLL(None, use_errno=True)
# Changes are observed after writes/atomic moves, not while an import is incomplete.
MASK = 0x8 | 0x40 | 0x80 | 0x100 | 0x200 | 0x400 | 0x800
ISDIR = 0x40000000
IGNORED = 0x8000


def flavour(path):
    try:
        data = json.loads(path.read_text())
        return tuple(
            data.get(key, default)
            for key, default in (
                ("flavour", "default"),
                ("mode", "dark"),
                ("variant", "tonalspot"),
            )
        )
    except (OSError, ValueError):
        return ("default", "dark", "tonalspot")


def watch(root, scheme, engine=None):
    engine = (
        engine
        or Path.home() / ".local/share/nacre/palette-runtime/venv/bin/nacre_shell"
    )
    root.mkdir(parents=True, exist_ok=True)
    scheme.parent.mkdir(parents=True, exist_ok=True)
    fd = LIBC.inotify_init1(os.O_NONBLOCK | os.O_CLOEXEC)
    if fd < 0:
        raise OSError(ctypes.get_errno(), "Cannot watch wallpaper directories")
    paths = {}

    def rescan():
        wanted = {
            root,
            scheme.parent,
            engine.parent,
            root.parent,
            Path.home() / ".config/nacre",
            *(p for p in root.rglob("*") if p.is_dir() and not p.is_symlink()),
        }
        for wd, path in list(paths.items()):
            if path not in wanted:
                LIBC.inotify_rm_watch(fd, wd)
                paths.pop(wd, None)
        known = set(paths.values())
        for path in wanted - known:
            wd = LIBC.inotify_add_watch(fd, os.fsencode(path), MASK)
            if wd >= 0:
                paths[wd] = path

    rescan()
    previous = flavour(scheme)
    deadline = None
    with selectors.DefaultSelector() as selector:
        selector.register(fd, selectors.EVENT_READ)
        try:
            while True:
                events = selector.select(
                    None if deadline is None else max(0, deadline - time.monotonic())
                )
                if events:
                    raw = os.read(fd, 262144)
                    offset = 0
                    dirty = False
                    while offset < len(raw):
                        wd, mask, _, length = struct.unpack_from("iIII", raw, offset)
                        name = os.fsdecode(
                            raw[offset + 16 : offset + 16 + length].split(b"\0", 1)[0]
                        )
                        offset += 16 + length
                        path = paths.get(wd)
                        if (
                            mask & 0x4000
                        ):  # Event queue overflow: rebuild from disk once.
                            dirty = True
                            continue
                        if mask & IGNORED:
                            paths.pop(wd, None)
                            continue
                        if path == engine.parent:
                            dirty |= name == engine.name
                        elif path == scheme.parent:
                            if name == scheme.name:
                                current = flavour(scheme)
                                dirty |= current != previous
                                previous = current
                        elif path == Path.home() / ".config/nacre":
                            dirty |= name in ("cli.json", "wallpaper-picker.json")
                        elif path == root.parent:
                            if name == root.name:
                                dirty = True
                                rescan()
                        elif path and (
                            mask & ISDIR
                            or Path(name).suffix.lower()
                            in (
                                ".jpg",
                                ".jpeg",
                                ".png",
                                ".webp",
                                ".gif",
                                ".tif",
                                ".tiff",
                                ".mp4",
                                ".webm",
                                ".mkv",
                                ".mov",
                                ".avi",
                                ".m4v",
                            )
                        ):
                            if mask & ISDIR:
                                rescan()
                            if mask & (0x8 | 0x40 | 0x80 | 0x200 | 0x400 | 0x800):
                                dirty = True
                    if dirty:
                        deadline = time.monotonic() + 0.5
                if deadline is not None and time.monotonic() >= deadline:
                    rescan()
                    print(json.dumps({"changed": True}), flush=True)
                    deadline = None
        finally:
            os.close(fd)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--root", type=Path, default=Path.home() / "Pictures/Wallpapers"
    )
    parser.add_argument(
        "--scheme",
        type=Path,
        default=Path.home() / ".local/state/nacre/scheme.json",
    )
    parser.add_argument("--engine", type=Path)
    args = parser.parse_args()
    watch(args.root, args.scheme, args.engine)
