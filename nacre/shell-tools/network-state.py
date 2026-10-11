#!/usr/bin/env python3
"""Read-only, bounded NetworkManager snapshot for Nacre's event-driven service."""

import json
import os
import re
import subprocess
import sys


class SnapshotError(RuntimeError):
    pass


def fields(line):
    """Decode nmcli's documented colon/backslash escaping without shell parsing."""
    result = []
    word = []
    escaped = False
    for char in line:
        if escaped:
            if char not in (":", "\\"):
                raise ValueError("Unknown nmcli escape")
            word.append(char)
            escaped = False
        elif char == "\\":
            escaped = True
        elif char == ":":
            result.append("".join(word))
            word = []
        else:
            word.append(char)
    if escaped:
        raise ValueError("Incomplete nmcli escape")
    result.append("".join(word))
    return result


def access_points(text):
    points = []
    seen = set()
    for line in text.splitlines()[:4096]:
        try:
            active, ssid, bssid, signal, frequency, interface = fields(line)
            signal = int(signal)
            frequency = int(frequency.split()[0])
            if (
                active not in ("*", "")
                or len(ssid.encode("utf-8")) > 32
                or any(ord(char) < 32 for char in ssid)
                or not re.fullmatch(r"(?:[0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}", bssid)
                or not 0 <= signal <= 100
                or not 1 <= frequency <= 100000
                or not re.fullmatch(r"[A-Za-z0-9_][A-Za-z0-9_.-]{0,63}", interface)
            ):
                continue
        except (ValueError, IndexError):
            continue
        key = (bssid.upper(), interface)
        if key in seen:
            continue
        seen.add(key)
        points.append(
            dict(
                ssid=ssid,
                bssid=bssid.upper(),
                strength=signal,
                frequency=frequency,
                active=active == "*",
                interface=interface,
            )
        )
    return points


def connected_interface(text):
    for line in text.splitlines():
        try:
            interface, kind, state = fields(line)
        except ValueError:
            continue
        if (
            kind == "wifi"
            and state == "connected"
            and re.fullmatch(r"[A-Za-z0-9_][A-Za-z0-9_.-]{0,63}", interface)
        ):
            return interface
    return ""


def query(*arguments):
    try:
        result = subprocess.run(
            [
                "nmcli",
                "--terse",
                "--escape",
                "yes",
                "--colors",
                "no",
                "--wait",
                "5",
                *arguments,
            ],
            env={**os.environ, "LC_ALL": "C"},
            capture_output=True,
            text=True,
            timeout=8,
            check=True,
        )
    except (OSError, subprocess.SubprocessError) as error:
        raise SnapshotError(
            "NetworkManager is unavailable or did not respond."
        ) from error
    if len(result.stdout) > 2 * 1024 * 1024:
        raise SnapshotError("NetworkManager returned too much data.")
    return result.stdout


def snapshot():
    radio = query("radio", "wifi").strip()
    if radio not in ("enabled", "disabled"):
        raise SnapshotError("NetworkManager returned an unknown Wi-Fi radio state.")
    devices = query("--fields", "DEVICE,TYPE,STATE", "device", "status")
    points = query(
        "--fields",
        "IN-USE,SSID,BSSID,SIGNAL,FREQ,DEVICE",
        "device",
        "wifi",
        "list",
        "--rescan",
        "no",
    )
    return {
        "wifiEnabled": radio == "enabled",
        "wifiInterface": connected_interface(devices),
        "networks": access_points(points),
    }


def main():
    try:
        print(json.dumps(snapshot(), ensure_ascii=True))
    except SnapshotError as error:
        print(json.dumps({"error": str(error)}))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
