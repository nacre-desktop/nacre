"""Developer-only Xlib client for creating windows and inspecting raw hints."""

import ctypes as c
import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "width_fixture_values", ROOT / "nacre/shell-tools/tests/native_niri_widths.py"
)
values = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(values)
Pair, SizeHints = values.Pair, values.SizeHints
MIN_SIZE, MAX_SIZE = values.MIN_SIZE, values.MAX_SIZE


class X11:
    def __init__(self, display=None):
        self.lib = c.CDLL("libX11.so.6")
        signatures = {
            "XOpenDisplay": ([c.c_char_p], c.c_void_p),
            "XDefaultRootWindow": ([c.c_void_p], c.c_ulong),
            "XInternAtom": ([c.c_void_p, c.c_char_p, c.c_int], c.c_ulong),
            "XGetWMNormalHints": (
                [c.c_void_p, c.c_ulong, c.POINTER(SizeHints), c.POINTER(c.c_long)],
                c.c_int,
            ),
            "XSetWMNormalHints": (
                [c.c_void_p, c.c_ulong, c.POINTER(SizeHints)],
                c.c_int,
            ),
            "XFlush": ([c.c_void_p], c.c_int),
            "XSync": ([c.c_void_p, c.c_int], c.c_int),
            "XCloseDisplay": ([c.c_void_p], c.c_int),
            "XCreateSimpleWindow": (
                [
                    c.c_void_p,
                    c.c_ulong,
                    c.c_int,
                    c.c_int,
                    c.c_uint,
                    c.c_uint,
                    c.c_uint,
                    c.c_ulong,
                    c.c_ulong,
                ],
                c.c_ulong,
            ),
            "XStoreName": ([c.c_void_p, c.c_ulong, c.c_char_p], c.c_int),
            "XChangeProperty": (
                [
                    c.c_void_p,
                    c.c_ulong,
                    c.c_ulong,
                    c.c_ulong,
                    c.c_int,
                    c.c_int,
                    c.c_void_p,
                    c.c_int,
                ],
                c.c_int,
            ),
            "XMapWindow": ([c.c_void_p, c.c_ulong], c.c_int),
        }
        for name, (args, result) in signatures.items():
            function = getattr(self.lib, name)
            function.argtypes, function.restype = args, result
        self.display = self.lib.XOpenDisplay(display.encode() if display else None)
        if not self.display:
            raise RuntimeError("Cannot connect to private X11 fixture display")
        self.root = self.lib.XDefaultRootWindow(self.display)

    def atom(self, name):
        return self.lib.XInternAtom(self.display, name.encode(), 0)

    def hints(self, window):
        value, supplied = SizeHints(), c.c_long()
        return (
            value
            if self.lib.XGetWMNormalHints(
                self.display, window, c.byref(value), c.byref(supplied)
            )
            else None
        )

    def set_hints(self, window, hints):
        self.lib.XSetWMNormalHints(self.display, window, c.byref(hints))
        self.lib.XSync(self.display, 0)

    def create(self, title, minimum):
        window = self.lib.XCreateSimpleWindow(
            self.display, self.root, 0, 0, 500, 500, 0, 0, 0
        )
        self.lib.XStoreName(self.display, window, title.encode())
        classes = c.create_string_buffer(b"fixture\0Fixture\0")
        self.lib.XChangeProperty(
            self.display,
            window,
            self.atom("WM_CLASS"),
            self.atom("STRING"),
            8,
            0,
            classes,
            len(classes.raw) - 1,
        )
        hints = SizeHints()
        hints.flags = 1023
        hints.min_width, hints.min_height = minimum, 600
        hints.max_width, hints.max_height = 4000, 2000
        hints.width_inc, hints.height_inc = 10, 20
        hints.base_width, hints.base_height = 30, 40
        hints.min_aspect, hints.max_aspect = Pair(4, 3), Pair(16, 9)
        hints.win_gravity = 1
        self.set_hints(window, hints)
        self.lib.XMapWindow(self.display, window)
        self.lib.XSync(self.display, 0)
        return window

    def close(self):
        self.lib.XCloseDisplay(self.display)
