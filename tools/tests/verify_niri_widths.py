#!/usr/bin/env python3
"""Exercise tiled X11 width hints in a private stock Niri desktop."""

import argparse
import ctypes as c
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "target/release/nacre-helper"
import widths_x11_fixture as widths


def await_value(read, accept, timeout=5):
    deadline = time.monotonic() + timeout
    value = None
    while time.monotonic() < deadline:
        value = read()
        if accept(value):
            return value
        time.sleep(0.05)
    raise RuntimeError("Native width verification timed out: " + str(value))


def verify(output):
    report = {}
    with tempfile.TemporaryDirectory(prefix="nwidth-") as folder:
        root = Path(folder)
        runtime = root / "runtime"
        runtime.mkdir(mode=0o700)
        display = Path(os.environ["WAYLAND_DISPLAY"])
        if not display.is_absolute():
            display = Path(os.environ["XDG_RUNTIME_DIR"]) / display
        config = root / "config.kdl"
        config.write_text(
            "layout { gaps 12; default-column-width { proportion 0.5; }; }\nhotkey-overlay { skip-at-startup; }\n"
        )
        env = dict(
            os.environ,
            XDG_RUNTIME_DIR=str(runtime),
            WAYLAND_DISPLAY=str(display),
            NACRE_COMPOSITOR="niri",
        )
        live_env = dict(os.environ)
        for key in ("NIRI_SOCKET", "DISPLAY", "NOTIFY_SOCKET", "NACRE_QS_CONFIG"):
            env.pop(key, None)
        with (root / "nested.log").open("wb") as log:
            compositor = subprocess.Popen(
                ["/usr/bin/niri", "-c", str(config)], env=env, stdout=log, stderr=log
            )
            helper, x11 = None, None
            old_display = os.environ.get("DISPLAY")
            try:
                ipc = await_value(
                    lambda: list(runtime.glob(f"niri.*.{compositor.pid}.sock")), bool
                )[0]
                xdisplay = await_value(
                    lambda: re.findall(
                        r"listening on X11 socket: (:[0-9]+)",
                        (root / "nested.log").read_text(),
                    ),
                    bool,
                )[0]
                env.update(NIRI_SOCKET=str(ipc), DISPLAY=xdisplay)
                os.environ["DISPLAY"] = xdisplay
                x11 = widths.X11()
                x = x11.lib
                x.XCreateSimpleWindow.argtypes = [
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
                x.XCreateSimpleWindow.restype = c.c_ulong
                x.XStoreName.argtypes = [c.c_void_p, c.c_ulong, c.c_char_p]
                x.XChangeProperty.argtypes = [
                    c.c_void_p,
                    c.c_ulong,
                    c.c_ulong,
                    c.c_ulong,
                    c.c_int,
                    c.c_int,
                    c.c_void_p,
                    c.c_int,
                ]
                x.XMapWindow.argtypes = [c.c_void_p, c.c_ulong]

                def create(title, minimum, fixed=False, dialog=False):
                    window = x.XCreateSimpleWindow(
                        x11.display, x11.root, 0, 0, 500, 500, 0, 0, 0x294034
                    )
                    x.XStoreName(x11.display, window, title.encode())
                    classes = c.create_string_buffer(
                        b"nacre-width-probe\0NacreWidthProbe\0"
                    )
                    x.XChangeProperty(
                        x11.display,
                        window,
                        x11.atom("WM_CLASS"),
                        x11.atom("STRING"),
                        8,
                        0,
                        classes,
                        len(classes.raw) - 1,
                    )
                    kind = c.c_ulong(
                        x11.atom(
                            "_NET_WM_WINDOW_TYPE_DIALOG"
                            if dialog
                            else "_NET_WM_WINDOW_TYPE_NORMAL"
                        )
                    )
                    x.XChangeProperty(
                        x11.display,
                        window,
                        x11.atom("_NET_WM_WINDOW_TYPE"),
                        x11.atom("ATOM"),
                        32,
                        0,
                        c.byref(kind),
                        1,
                    )
                    hints = widths.SizeHints()
                    hints.flags = widths.MIN_SIZE
                    hints.min_width, hints.min_height = minimum, 123
                    if fixed:
                        hints.flags |= widths.MAX_SIZE
                        hints.max_width, hints.max_height = minimum, 500
                    x11.set_hints(window, hints)
                    x.XMapWindow(x11.display, window)
                    x.XFlush(x11.display)
                    return window

                def msg(*args):
                    return subprocess.check_output(
                        ["/usr/bin/niri", "msg", *args], env=env, text=True
                    )

                def native(title):
                    return next(
                        (
                            w
                            for w in json.loads(msg("-j", "windows"))
                            if w["title"] == title
                        ),
                        None,
                    )

                def live(*args):
                    return subprocess.check_output(
                        ["/usr/bin/niri", "msg", *args], env=live_env, text=True
                    )

                baseline = create("Before helper", 1200)
                initial = await_value(lambda: native("Before helper"), bool)
                report["stockMinimumWidth"] = initial["layout"]["window_size"][0]
                assert report["stockMinimumWidth"] == 1200
                helper = subprocess.Popen(
                    [str(HELPER), "niri_widths"], env=env, stdout=log, stderr=log
                )
                await_value(
                    lambda: x11.hints(baseline).min_width, lambda value: value == 1
                )
                normal = create("New tiled window", 2400)
                await_value(
                    lambda: x11.hints(normal).min_width, lambda value: value == 1
                )
                tiled = await_value(lambda: native("New tiled window"), bool)
                time.sleep(0.6)
                tiled = native("New tiled window")
                report["newWindowWidth"] = tiled["layout"]["window_size"][0]
                report["outputs"] = json.loads(msg("-j", "outputs"))
                create("Half-width reference", 0)
                await_value(lambda: native("Half-width reference"), bool)
                assert (
                    report["newWindowWidth"]
                    == native("Half-width reference")["layout"]["window_size"][0]
                )
                outer = await_value(
                    lambda: next(
                        (
                            w
                            for w in json.loads(live("-j", "windows"))
                            if w["pid"] == compositor.pid
                        ),
                        None,
                    ),
                    bool,
                )
                live(
                    "action",
                    "move-window-to-workspace",
                    "--window-id",
                    str(outer["id"]),
                    "--focus",
                    "false",
                    "5",
                )
                report["outputWidths"] = []
                for width in (600, 1000):
                    before = next(iter(json.loads(msg("-j", "outputs")).values()))[
                        "logical"
                    ]["width"]
                    live(
                        "action",
                        "set-window-width",
                        "--id",
                        str(outer["id"]),
                        str(width),
                    )
                    await_value(
                        lambda: next(iter(json.loads(msg("-j", "outputs")).values()))[
                            "logical"
                        ]["width"],
                        lambda v, before=before: v != before,
                    )
                    time.sleep(0.2)
                    current = native("New tiled window")["layout"]["window_size"][0]
                    assert (
                        current
                        == native("Half-width reference")["layout"]["window_size"][0]
                    )
                    report["outputWidths"].append(
                        {
                            "logicalOutputWidth": next(
                                iter(json.loads(msg("-j", "outputs")).values())
                            )["logical"]["width"],
                            "tileWidth": current,
                        }
                    )
                for minimum in (800, 1600, 2400):
                    hints = x11.hints(normal)
                    hints.min_width = minimum
                    x11.set_hints(normal, hints)
                    await_value(lambda: x11.hints(normal).min_width, lambda v: v == 1)
                    time.sleep(0.1)
                    assert (
                        native("New tiled window")["layout"]["window_size"][0]
                        == native("Half-width reference")["layout"]["window_size"][0]
                    )
                report["changingMinimumKeepsProportion"] = True
                msg("action", "set-window-width", "--id", str(tiled["id"]), "500")
                await_value(
                    lambda: native("New tiled window")["layout"]["window_size"][0],
                    lambda value: value == 500,
                )
                updated = x11.hints(normal)
                updated.min_width = 2800
                x11.set_hints(normal, updated)
                await_value(
                    lambda: x11.hints(normal).min_width, lambda value: value == 1
                )
                time.sleep(0.2)
                report["manualWidthPreserved"] = (
                    native("New tiled window")["layout"]["window_size"][0] == 500
                )
                assert report["manualWidthPreserved"]
                msg("action", "toggle-window-floating", "--id", str(tiled["id"]))
                await_value(
                    lambda: x11.hints(normal).min_width, lambda value: value == 2800
                )
                report["floatingRestoresLatestMinimum"] = True
                msg("action", "toggle-window-floating", "--id", str(tiled["id"]))
                await_value(
                    lambda: x11.hints(normal).min_width, lambda value: value == 1
                )
                fixed = create("Fixed width", 600, fixed=True)
                dialog = create("Dialog", 600, dialog=True)
                await_value(lambda: native("Fixed width"), bool)
                await_value(lambda: native("Dialog"), bool)
                time.sleep(0.2)
                report["fixedWidthPreserved"] = x11.hints(fixed).min_width == 600
                report["dialogPreserved"] = x11.hints(dialog).min_width == 600
                assert report["fixedWidthPreserved"] and report["dialogPreserved"]
                assert x11.hints(normal).min_height == 123
                helper.terminate()
                helper.wait(timeout=5)
                report["shutdownRestoresMinimum"] = x11.hints(normal).min_width == 2800
                assert report["shutdownRestoresMinimum"]
                report["nativeVerified"] = True
            finally:
                if helper and helper.poll() is None:
                    helper.terminate()
                    helper.wait(timeout=5)
                if x11:
                    x11.close()
                if old_display is None:
                    os.environ.pop("DISPLAY", None)
                else:
                    os.environ["DISPLAY"] = old_display
                compositor.terminate()
                compositor.wait(timeout=5)
    Path(output).write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    verify(parser.parse_args().output)
