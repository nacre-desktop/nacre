"""Relax advisory X11 minimum widths only for ordinary Niri tiled windows.

New constrained windows receive one proportional opening-width correction.
Later geometry, application zoom, maximum sizes and height hints stay intact.
The observer starts only from main(), never when imported.
"""

import argparse
import ctypes as c
import json
import math
import os
from pathlib import Path
import selectors
import signal
import socket
import sys
import time

MIN_SIZE = 1 << 4
MAX_SIZE = 1 << 5
PROPERTY_CHANGE = 1 << 22
SUBSTRUCTURE_NOTIFY = 1 << 19
MAX_BYTES = 4 * 1024 * 1024


class Pair(c.Structure):
    _fields_ = [("x", c.c_int), ("y", c.c_int)]


class SizeHints(c.Structure):
    _fields_ = (
        [("flags", c.c_long)]
        + [
            (name, c.c_int)
            for name in (
                "x",
                "y",
                "width",
                "height",
                "min_width",
                "min_height",
                "max_width",
                "max_height",
                "width_inc",
                "height_inc",
            )
        ]
        + [("min_aspect", Pair), ("max_aspect", Pair)]
        + [(name, c.c_int) for name in ("base_width", "base_height", "win_gravity")]
    )


class X11:
    def __init__(self):
        self.lib = x = c.CDLL("libX11.so.6")
        signatures = {
            "XOpenDisplay": ([c.c_char_p], c.c_void_p),
            "XDefaultRootWindow": ([c.c_void_p], c.c_ulong),
            "XConnectionNumber": ([c.c_void_p], c.c_int),
            "XInternAtom": ([c.c_void_p, c.c_char_p, c.c_int], c.c_ulong),
            "XQueryTree": (
                [
                    c.c_void_p,
                    c.c_ulong,
                    c.POINTER(c.c_ulong),
                    c.POINTER(c.c_ulong),
                    c.POINTER(c.POINTER(c.c_ulong)),
                    c.POINTER(c.c_uint),
                ],
                c.c_int,
            ),
            "XGetWindowProperty": (
                [
                    c.c_void_p,
                    c.c_ulong,
                    c.c_ulong,
                    c.c_long,
                    c.c_long,
                    c.c_int,
                    c.c_ulong,
                    c.POINTER(c.c_ulong),
                    c.POINTER(c.c_int),
                    c.POINTER(c.c_ulong),
                    c.POINTER(c.c_ulong),
                    c.POINTER(c.c_void_p),
                ],
                c.c_int,
            ),
            "XGetWMNormalHints": (
                [c.c_void_p, c.c_ulong, c.POINTER(SizeHints), c.POINTER(c.c_long)],
                c.c_int,
            ),
            "XSetWMNormalHints": (
                [c.c_void_p, c.c_ulong, c.POINTER(SizeHints)],
                c.c_int,
            ),
            "XSelectInput": ([c.c_void_p, c.c_ulong, c.c_long], c.c_int),
            "XPending": ([c.c_void_p], c.c_int),
            "XNextEvent": ([c.c_void_p, c.c_void_p], c.c_int),
            "XFlush": ([c.c_void_p], c.c_int),
            "XFree": ([c.c_void_p], c.c_int),
            "XCloseDisplay": ([c.c_void_p], c.c_int),
        }
        for name, (arguments, result) in signatures.items():
            fn = getattr(x, name)
            fn.argtypes, fn.restype = arguments, result
        # A client may disappear between discovery and a property query.
        self.error_handler = c.CFUNCTYPE(c.c_int, c.c_void_p, c.c_void_p)(lambda *_: 0)
        x.XSetErrorHandler.argtypes = [type(self.error_handler)]
        x.XSetErrorHandler(self.error_handler)
        self.display = x.XOpenDisplay(None)
        if not self.display:
            raise RuntimeError("Cannot connect to the session X11 display")
        self.root = x.XDefaultRootWindow(self.display)
        self.atoms = {}
        self.selected = set()

    def fileno(self):
        return self.lib.XConnectionNumber(self.display)

    def atom(self, name):
        if name not in self.atoms:
            self.atoms[name] = self.lib.XInternAtom(self.display, name.encode(), 0)
        return self.atoms[name]

    def property(self, window, name):
        kind, fmt, count, remaining, data = (
            c.c_ulong(),
            c.c_int(),
            c.c_ulong(),
            c.c_ulong(),
            c.c_void_p(),
        )
        status = self.lib.XGetWindowProperty(
            self.display,
            window,
            self.atom(name),
            0,
            4096,
            0,
            0,
            c.byref(kind),
            c.byref(fmt),
            c.byref(count),
            c.byref(remaining),
            c.byref(data),
        )
        if status or not data:
            return None
        try:
            if remaining.value:
                return None
            if fmt.value == 8:
                return c.string_at(data, count.value)
            if fmt.value == 32:
                values = c.cast(data, c.POINTER(c.c_ulong))
                return tuple(values[i] for i in range(count.value))
            return None
        finally:
            self.lib.XFree(data)

    def windows(self):
        found = []

        def visit(window, depth=0):
            found.append(window)
            if window not in self.selected:
                self.lib.XSelectInput(
                    self.display, window, PROPERTY_CHANGE | SUBSTRUCTURE_NOTIFY
                )
                self.selected.add(window)
            if depth >= 4:
                return
            root, parent, children, count = (
                c.c_ulong(),
                c.c_ulong(),
                c.POINTER(c.c_ulong)(),
                c.c_uint(),
            )
            if self.lib.XQueryTree(
                self.display,
                window,
                c.byref(root),
                c.byref(parent),
                c.byref(children),
                c.byref(count),
            ):
                values = [children[i] for i in range(count.value)]
                self.lib.XFree(children)
                for child in values:
                    visit(child, depth + 1)

        visit(self.root)
        self.selected.intersection_update(found)
        return found

    def identity(self, window):
        classes = self.property(window, "WM_CLASS")
        title = self.property(window, "_NET_WM_NAME") or self.property(
            window, "WM_NAME"
        )
        if not isinstance(classes, bytes) or not isinstance(title, bytes):
            return None
        return {
            v.decode(errors="replace").casefold() for v in classes.split(b"\0") if v
        }, title.decode(errors="replace").rstrip("\0")

    def ordinary(self, window):
        if self.property(window, "WM_TRANSIENT_FOR"):
            return False
        kinds = self.property(window, "_NET_WM_WINDOW_TYPE")
        return not kinds or self.atom("_NET_WM_WINDOW_TYPE_NORMAL") in kinds

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
        self.lib.XFlush(self.display)

    def drain(self):
        event = (c.c_long * 24)()
        changed = False
        while self.lib.XPending(self.display):
            self.lib.XNextEvent(self.display, c.byref(event))
            if c.cast(c.byref(event), c.POINTER(c.c_int))[0] in (16, 17, 21):
                self.selected.clear()
            changed = True
        return changed

    def close(self):
        self.lib.XCloseDisplay(self.display)


class Policy:
    def __init__(self, x11):
        self.x11 = x11
        self.windows = {}
        self.saved = {}
        self.mapping = {}
        self.classes = set()
        self.initialized = False
        self.new = set()
        self.opening = {}
        self.eligible = set()

    def event(self, event):
        if "WindowsChanged" in event:
            windows = {w["id"]: w for w in event["WindowsChanged"]["windows"]}
            if self.initialized:
                self.new.update(windows.keys() - self.windows.keys())
            self.windows = windows
            self.initialized = True
            return True
        if "WindowOpenedOrChanged" in event:
            w = event["WindowOpenedOrChanged"]["window"]
            previous = self.windows.get(w["id"], {})
            if self.initialized and w["id"] not in self.windows:
                self.new.add(w["id"])
            self.windows[w["id"]] = w
            return any(
                (item.get("app_id") or "").casefold() in self.classes
                for item in (previous, w)
            )
        if "WindowClosed" in event:
            identifier = event["WindowClosed"]["id"]
            previous = self.windows.pop(identifier, {})
            self.new.discard(identifier)
            self.opening.pop(identifier, None)
            return (previous.get("app_id") or "").casefold() in self.classes
        return False

    def tiled(self, identity, window=None):
        if identity is None:
            return False
        classes, title = identity
        matches = [
            w
            for w in self.windows.values()
            if (w.get("app_id") or "").casefold() in classes and w.get("title") == title
        ]
        previous = self.mapping.get(window, set())
        # A proven identity survives asynchronous X11/Niri title updates.
        matches += [
            self.windows[i]
            for i in previous
            if i in self.windows
            and (self.windows[i].get("app_id") or "").casefold() in classes
        ]
        if matches and window is not None:
            self.mapping[window] = {w["id"] for w in matches}
        # Ambiguous mixed tiled/floating windows keep their original constraints.
        return bool(matches) and all(w.get("is_floating") is False for w in matches)

    def restore(self, window):
        previous = self.saved.pop(window, None)
        hints = self.x11.hints(window)
        if (
            previous is not None
            and hints is not None
            and hints.flags & MIN_SIZE
            and hints.min_width == 1
        ):
            hints.min_width = (
                min(previous, hints.max_width)
                if hints.flags & MAX_SIZE and hints.max_width > 0
                else previous
            )
            self.x11.set_hints(window, hints)

    def reconcile(self):
        present = set(self.x11.windows())
        identities = {window: self.x11.identity(window) for window in present}
        self.classes = set().union(
            *(identity[0] for identity in identities.values() if identity is not None)
        )
        self.eligible.clear()
        for window in self.saved.keys() - present:
            self.saved.pop(window)
        for window in self.mapping.keys() - present:
            self.mapping.pop(window)
        for window in present:
            if not self.x11.ordinary(window) or not self.tiled(
                identities[window], window
            ):
                if window in self.saved:
                    self.restore(window)
                continue
            hints = self.x11.hints(window)
            if hints is None or not hints.flags & MIN_SIZE:
                self.saved.pop(window, None)
                continue
            if hints.flags & MAX_SIZE and hints.min_width == hints.max_width:
                if window in self.saved:
                    self.restore(window)
                continue
            self.eligible.update(self.mapping.get(window, set()))
            if hints.min_width > 1:
                self.saved[window] = hints.min_width
                hints.min_width = 1
                self.x11.set_hints(window, hints)
                for identifier in self.mapping.get(window, set()) & self.new:
                    # Let the bridge forward the new hints before correcting
                    # the app's already-committed oversized initial surface.
                    self.opening[identifier] = time.monotonic() + 0.15
                    self.new.discard(identifier)

    def apply_openings(self, resize, now=None):
        now = time.monotonic() if now is None else now
        for identifier, deadline in list(self.opening.items()):
            if deadline > now:
                continue
            self.opening.pop(identifier)
            window = self.windows.get(identifier, {})
            if identifier in self.eligible and window.get("is_floating") is False:
                resize(identifier)

    def close(self):
        for window in list(self.saved):
            self.restore(window)


def check_session():
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
    from compositor import select_backend

    return (
        select_backend() == "niri"
        and bool(os.environ.get("NIRI_SOCKET"))
        and bool(os.environ.get("DISPLAY"))
    )


def run(opening_width):
    from compositor_backends.niri import Backend

    backend = Backend()

    def resize(identifier):
        try:
            backend._action(
                "SetWindowWidth",
                {"id": identifier, "change": {"SetProportion": opening_width}},
                timeout=2,
            )
        except (OSError, RuntimeError, ValueError):
            # A window can close after its event was received.
            pass

    x11 = X11()
    policy = Policy(x11)
    connection = socket.socket(socket.AF_UNIX)
    connection.connect(os.environ["NIRI_SOCKET"])
    connection.sendall(b'"EventStream"\n')
    connection.setblocking(False)
    wake_read, wake_write = os.pipe2(os.O_NONBLOCK | os.O_CLOEXEC)
    stop = False

    def shutdown(*_):
        nonlocal stop
        stop = True
        os.write(wake_write, b"\0")

    signal.signal(signal.SIGTERM, shutdown)
    signal.signal(signal.SIGINT, shutdown)
    buffer = b""
    with selectors.DefaultSelector() as selector:
        selector.register(connection, selectors.EVENT_READ, "niri")
        selector.register(x11.fileno(), selectors.EVENT_READ, "x11")
        selector.register(wake_read, selectors.EVENT_READ, "stop")
        try:
            policy.reconcile()
            while not stop:
                policy.apply_openings(resize)
                if x11.drain():
                    policy.reconcile()
                    continue
                changed = False
                timeout = (
                    max(0, min(policy.opening.values()) - time.monotonic())
                    if policy.opening
                    else None
                )
                for key, _ in selector.select(timeout):
                    if key.data == "stop":
                        stop = True
                    elif key.data == "x11":
                        changed = x11.drain() or changed
                    else:
                        chunk = connection.recv(65536)
                        if not chunk:
                            return
                        buffer += chunk
                        if len(buffer) > MAX_BYTES:
                            raise ValueError("Oversized Niri event stream message")
                        while b"\n" in buffer:
                            line, buffer = buffer.split(b"\n", 1)
                            event = json.loads(line)
                            if "Err" in event:
                                raise ValueError("Niri rejected the event stream")
                            changed = policy.event(event) or changed
                if changed:
                    policy.reconcile()
        finally:
            policy.close()
            connection.close()
            os.close(wake_read)
            os.close(wake_write)
            x11.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check-session", action="store_true")
    parser.add_argument(
        "--opening-width",
        type=float,
        default=50,
        help="Initial width percentage for newly constrained tiled X11 windows",
    )
    args = parser.parse_args()
    if not math.isfinite(args.opening_width) or not 0 < args.opening_width <= 100:
        parser.error("Opening width must be greater than zero and at most 100 percent")
    supported = check_session()
    if args.check_session:
        return 0 if supported else 1
    if supported:
        run(args.opening_width)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
