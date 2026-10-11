#!/usr/bin/env python3
"""Bounded light discovery/actions; native QML owns reads and write coalescing."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

SYS = Path("/sys")


def integer(path):
    try:
        value = int(path.read_text().strip())
        return value if value >= 0 else None
    except (OSError, ValueError):
        return None


def sys_device(directory, kind):
    maximum = integer(directory / "max_brightness")
    current = integer(directory / "brightness")
    if not maximum or current is None or current > maximum:
        return None
    return dict(kind=kind, device=directory.name, maximum=maximum, current=current)


def fingerprint(connector):
    try:
        data = (connector / "edid").read_bytes()
        return hashlib.sha256(data).hexdigest() if data else ""
    except OSError:
        return ""


def ddc_read(bus):
    result = subprocess.run(
        ["ddcutil", "--bus", str(bus), "--brief", "getvcp", "10"],
        capture_output=True,
        text=True,
        check=True,
        timeout=6,
    )
    match = re.search(r"^VCP\s+10\s+C\s+(\d+)\s+(\d+)\s*$", result.stdout, re.M)
    if not match or not 0 <= int(match[1]) <= int(match[2]) or int(match[2]) <= 0:
        raise ValueError("Display brightness response was invalid")
    return int(match[1]), int(match[2])


def discover(screens, root=SYS):
    panels = []
    for path in sorted((root / "class/backlight").glob("*")):
        row = sys_device(path, "backlight")
        if row:
            panels.append(row)
    keyboard = next(
        (
            row
            for path in sorted((root / "class/leds").glob("*kbd_backlight*"))
            if (row := sys_device(path, "keyboard"))
        ),
        None,
    )
    externals = []
    for name in screens:
        if (
            not isinstance(name, str)
            or not re.fullmatch(r"[A-Za-z0-9_.-]{1,64}", name)
            or re.match(r"^(eDP|LVDS)", name, re.I)
        ):
            continue
        matches = []
        for connector in (root / "class/drm").glob("card*-" + name):
            try:
                if (connector / "status").read_text().strip() != "connected":
                    continue
                bus = re.fullmatch(r"i2c-(\d+)", (connector / "ddc").resolve().name)
                if bus:
                    matches.append((connector, int(bus[1])))
            except OSError:
                continue
        if len(matches) != 1:
            continue
        connector, bus = matches[0]
        identity = fingerprint(connector)
        if not identity:
            continue
        try:
            current, maximum = ddc_read(bus)
        except (OSError, ValueError, subprocess.SubprocessError):
            continue
        externals.append(
            dict(
                kind="ddc",
                screen=name,
                bus=bus,
                connector=connector.name,
                identity=identity,
                current=current,
                maximum=maximum,
            )
        )
    return dict(
        backlight=panels[0] if panels else None, keyboard=keyboard, ddc=externals
    )


def write(descriptor, value, root=SYS):
    if not isinstance(descriptor, dict) or type(value) is not int:
        raise ValueError("Invalid brightness request")
    kind = descriptor.get("kind")
    if kind in ("backlight", "keyboard"):
        device = descriptor.get("device", "")
        if (
            not isinstance(device, str)
            or not re.fullmatch(r"[A-Za-z0-9_.:+-]{1,128}", device)
            or (kind == "keyboard" and "kbd_backlight" not in device)
        ):
            raise ValueError("Invalid light device")
        path = (
            root / "class" / ("backlight" if kind == "backlight" else "leds") / device
        )
        current = sys_device(path, kind)
        if not current or current["maximum"] != descriptor.get("maximum"):
            raise ValueError("Light device changed or is unavailable")
        minimum = 1 if kind == "backlight" else 0
        value = max(minimum, min(current["maximum"], value))
        subprocess.run(
            [
                "brightnessctl",
                "--class",
                "backlight" if kind == "backlight" else "leds",
                "--device",
                device,
                "set",
                str(value),
            ],
            capture_output=True,
            text=True,
            check=True,
            timeout=6,
        )
        result = sys_device(path, kind)
        if not result:
            raise ValueError("Light device disappeared after the write")
        return result
    if kind == "ddc":
        bus = descriptor.get("bus")
        connector = descriptor.get("connector", "")
        if (
            type(bus) is not int
            or not 0 <= bus <= 65535
            or not isinstance(connector, str)
            or not re.fullmatch(r"card\d+-[A-Za-z0-9_.-]+", connector)
        ):
            raise ValueError("Invalid display control")
        path = root / "class/drm" / connector
        if (
            (path / "status").read_text().strip() != "connected"
            or (path / "ddc").resolve().name != "i2c-" + str(bus)
            or fingerprint(path) != descriptor.get("identity")
        ):
            raise ValueError("Display changed or disconnected")
        _, maximum = ddc_read(bus)
        if maximum != descriptor.get("maximum"):
            raise ValueError("Display brightness range changed")
        value = max(0, min(maximum, value))
        subprocess.run(
            ["ddcutil", "--bus", str(bus), "setvcp", "10", str(value)],
            capture_output=True,
            text=True,
            check=True,
            timeout=6,
        )
        current, maximum = ddc_read(bus)
        return dict(descriptor, current=current, maximum=maximum)
    raise ValueError("Unsupported light device")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["discover", "set", "read-ddc"])
    parser.add_argument("data")
    parser.add_argument("value", nargs="?", type=int)
    args = parser.parse_args()
    try:
        data = json.loads(args.data)
        if args.action == "discover":
            if not isinstance(data, list):
                raise ValueError("Invalid display list")
            result = discover(data)
        elif args.action == "set":
            result = write(data, args.value)
        else:
            if (
                not isinstance(data, dict)
                or data.get("kind") != "ddc"
                or type(data.get("bus")) is not int
                or not 0 <= data["bus"] <= 65535
            ):
                raise ValueError("Invalid display control")
            current, maximum = ddc_read(data["bus"])
            result = dict(data, current=current, maximum=maximum)
        print(json.dumps(result))
    except (OSError, ValueError, subprocess.SubprocessError):
        print(
            json.dumps(
                {
                    "error": "Light control failed; refresh the controls and check device access."
                }
            )
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
