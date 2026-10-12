#!/usr/bin/env python3
"""Finite compositor operations scoped to a Nacre-owned pearl window."""

import argparse
import json
import re
import subprocess
import time

TITLE = re.compile(r"Nacre Pearl (Launcher|Dashboard|AI|Controls|Media) [0-9]+")


def read(kind):
    return json.loads(
        subprocess.check_output(["hyprctl", "-j", kind], text=True, timeout=3)
    )


def owned(title, pid, clients):
    if not TITLE.fullmatch(title) or pid <= 0:
        raise ValueError("Invalid pearl identity")
    return next(
        (
            item
            for item in clients
            if item.get("title") == title and item.get("pid") == pid
        ),
        None,
    )


def dispatch(method, address, **values):
    if not re.fullmatch(r"0x[0-9a-fA-F]+", address):
        raise ValueError("Invalid owned window address")
    fields = {"window": "address:" + address, **values}
    params = ",".join(
        key
        + "="
        + (str(value).lower() if isinstance(value, bool) else json.dumps(value))
        for key, value in fields.items()
    )
    expression = (
        ("hl.dsp.focus" if method == "focus" else "hl.dsp.window." + method)
        + "({"
        + params
        + "})"
    )
    subprocess.run(
        ["hyprctl", "dispatch", expression], check=True, capture_output=True, timeout=3
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("geometry", "place", "fit", "pin", "focus"))
    parser.add_argument("title")
    parser.add_argument("pid", type=int)
    parser.add_argument("x", type=float, nargs="?", default=0)
    parser.add_argument("y", type=float, nargs="?", default=0)
    parser.add_argument("width", type=float, nargs="?", default=0)
    parser.add_argument("height", type=float, nargs="?", default=0)
    args = parser.parse_args()
    try:
        end = time.monotonic() + (4 if args.action == "place" else 0.2)
        client = None
        while time.monotonic() < end:
            client = owned(args.title, args.pid, read("clients"))
            if client:
                break
            time.sleep(0.025)
        if not client:
            raise ValueError("Pearl window not mapped")
        address = client["address"]
        if args.action == "place":
            dispatch("float", address, action="enable")
            if args.width > 0 and args.height > 0:
                dispatch(
                    "resize",
                    address,
                    x=round(args.width),
                    y=round(args.height),
                    relative=False,
                )
            dispatch("move", address, x=round(args.x), y=round(args.y), relative=False)
        elif args.action == "fit" and not client.get("fullscreen"):
            monitor = next(
                item for item in read("monitors") if item["id"] == client["monitor"]
            )
            mw, mh = monitor["width"], monitor["height"]
            if monitor.get("transform", 0) % 2:
                mw, mh = mh, mw
            scale = monitor.get("scale", 1) or 1
            left, top, right, bottom = monitor.get("reserved", [0, 0, 0, 0])
            maxw, maxh = mw / scale - left - right, mh / scale - top - bottom
            width = min(maxw, max(client["size"][0], args.width))
            height = min(maxh, max(client["size"][1], args.height))
            x = max(
                monitor["x"] + left,
                min(client["at"][0], monitor["x"] + mw / scale - right - width),
            )
            y = max(
                monitor["y"] + top,
                min(client["at"][1], monitor["y"] + mh / scale - bottom - height),
            )
            if [round(width), round(height)] != client["size"]:
                dispatch(
                    "resize", address, x=round(width), y=round(height), relative=False
                )
            if [round(x), round(y)] != client["at"]:
                dispatch("move", address, x=round(x), y=round(y), relative=False)
        elif args.action == "focus":
            dispatch("focus", address)
        elif args.action == "pin":
            if not client.get("floating"):
                raise ValueError("Float the window before pinning it")
            dispatch("pin", address)
        client = owned(args.title, args.pid, read("clients")) or client
        print(
            json.dumps(
                {
                    "address": address,
                    "x": client["at"][0],
                    "y": client["at"][1],
                    "width": client["size"][0],
                    "height": client["size"][1],
                }
            )
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(json.dumps({"error": str(error)[:200]}))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
