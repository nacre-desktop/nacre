#!/usr/bin/env python3
"""Freeze original Xlib hint decoding/setter on an explicitly private Xvfb."""

import argparse
import ctypes as c
import importlib.util
import json
import os
from pathlib import Path

import freeze_niri_widths as reference

DESTINATION = (
    reference.ROOT / "crates/nacre-helper/tests/fixtures/niri-widths-x11-reference.json"
)


def hint_values(widths, value):
    return (
        None
        if value is None
        else {
            name: {"x": item.x, "y": item.y} if isinstance(item, widths.Pair) else item
            for name, _ in widths.SizeHints._fields_
            for item in [getattr(value, name)]
        }
    )


def capture(destination):
    reference.verify_source()
    if os.environ.get("NACRE_PRIVATE_XVFB_REFERENCE") != "1":
        raise ValueError(
            "Use an explicit disposable HOME and private Xvfb reference display"
        )
    if destination.exists():
        raise ValueError("Refusing to overwrite frozen X11 hints")
    spec = importlib.util.spec_from_file_location(
        "current_width_xlib", reference.SOURCE
    )
    widths = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(widths)
    x11 = widths.X11()
    lib = x11.lib
    lib.XCreateSimpleWindow.argtypes = [
        c.c_void_p,
        c.c_ulong,
        c.c_int,
        c.c_int,
        c.c_uint,
        c.c_uint,
        c.c_uint,
        c.c_ulong,
        c.c_ulong,
    ]
    lib.XCreateSimpleWindow.restype = c.c_ulong
    lib.XChangeProperty.argtypes = [
        c.c_void_p,
        c.c_ulong,
        c.c_ulong,
        c.c_ulong,
        c.c_int,
        c.c_int,
        c.c_void_p,
        c.c_int,
    ]
    lib.XChangeProperty.restype = c.c_int
    lib.XDestroyWindow.argtypes = [c.c_void_p, c.c_ulong]
    lib.XSync.argtypes = [c.c_void_p, c.c_int]
    values = [48, 5, 7, 800, 600, 1600, 600, 4000, 2000, 10, 20, 4, 3, 16, 9, 30, 40, 1]
    requests = [
        {
            "name": f"length-{length}",
            "values": values[:length],
            "format": 32,
            "type": "WM_SIZE_HINTS",
        }
        for length in (0, 4, 14, 15, 16, 17, 18)
    ]
    requests += [
        {
            "name": "extra-fields",
            "values": values + [99, 100],
            "format": 32,
            "type": "WM_SIZE_HINTS",
        },
        {"name": "wrong-type", "values": values, "format": 32, "type": "STRING"},
        {
            "name": "wrong-format",
            "values": values,
            "format": 8,
            "type": "WM_SIZE_HINTS",
        },
    ]
    requests += [
        {
            "name": f"flags-{flags}",
            "values": [flags, *values[1:]],
            "format": 32,
            "type": "WM_SIZE_HINTS",
        }
        for flags in (0, 1, 2, 16, 32, 48, 255, 1023, 1 << 25)
    ]
    cases = []
    try:
        for request in requests:
            window = lib.XCreateSimpleWindow(
                x11.display, x11.root, 0, 0, 100, 100, 0, 0, 0
            )
            try:
                data = (
                    (c.c_ulong * len(request["values"]))(*request["values"])
                    if request["format"] == 32
                    else (c.c_ubyte * len(request["values"]))(
                        *[value & 255 for value in request["values"]]
                    )
                )
                lib.XChangeProperty(
                    x11.display,
                    window,
                    x11.atom("WM_NORMAL_HINTS"),
                    x11.atom(request["type"]),
                    request["format"],
                    0,
                    data,
                    len(request["values"]),
                )
                lib.XSync(x11.display, 0)
                hints = x11.hints(window)
                observed = {"hints": hint_values(widths, hints)}
                if hints is not None:
                    hints.min_width = 1
                    x11.set_hints(window, hints)
                    lib.XSync(x11.display, 0)
                    observed["written_property"] = list(
                        x11.property(window, "WM_NORMAL_HINTS")
                    )
                    observed["after"] = hint_values(widths, x11.hints(window))
                cases.append({"request": request, "observed": observed})
            finally:
                lib.XDestroyWindow(x11.display, window)
                lib.XSync(x11.display, 0)
    finally:
        x11.close()
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("x") as stream:
        json.dump(
            {
                "source_sha256": reference.SOURCE_SHA,
                "cases": cases,
                "stdout": "",
                "exit": 0,
                "files": {},
            },
            stream,
            indent=2,
        )
        stream.write("\n")
    print(f"Froze {len(cases)} original Xlib hint contracts")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=DESTINATION)
    capture(parser.parse_args().output)
