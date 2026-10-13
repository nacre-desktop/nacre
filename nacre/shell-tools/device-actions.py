#!/usr/bin/env python3
"""Bounded device actions; authentication stays in the native managers."""

import argparse
from nacre_i18n import tr
import json
import re
import subprocess
import os
import sys


def command(action, value=""):
    if action == "wifi-scan":
        # rescan only requests a scan; list waits for the fresh AP results.
        return [
            "nmcli",
            "--wait",
            "15",
            "--colors",
            "no",
            "--fields",
            "IN-USE",
            "device",
            "wifi",
            "list",
            "--rescan",
            "yes",
        ]
    if action == "wifi-radio" and value in ("on", "off"):
        return ["nmcli", "radio", "wifi", value]
    if action == "wifi-disconnect" and re.fullmatch(
        r"[A-Za-z0-9_][A-Za-z0-9_.-]{0,63}", value
    ):
        return ["nmcli", "device", "disconnect", value]
    if action == "bluetooth-power" and value in ("on", "off"):
        return ["bluetoothctl", "power", value]
    operations = {
        "bluetooth-connect": "connect",
        "bluetooth-disconnect": "disconnect",
        "bluetooth-trust": "trust",
        "bluetooth-untrust": "untrust",
    }
    if action in operations and re.fullmatch(
        r"(?:[0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}", value
    ):
        return ["bluetoothctl", operations[action], value]
    if (
        action == "wifi-connect"
        and value
        and len(value.encode("utf-8")) <= 32
        and not any(ord(c) < 32 for c in value)
    ):
        return ["nmcli", "--ask", "device", "wifi", "connect", value]
    raise ValueError(
        tr(
            "helper.device-actions.unsupported-device-action",
            "Unsupported device action",
        )
    )


def network_status():
    environment = dict(os.environ, LC_ALL="C")
    enabled = (
        subprocess.check_output(
            ["nmcli", "radio", "wifi"], text=True, timeout=5, env=environment
        ).strip()
        == "enabled"
    )
    devices = subprocess.check_output(
        ["nmcli", "-t", "-f", "DEVICE,TYPE,STATE", "device"],
        text=True,
        timeout=5,
        env=environment,
    )
    interface = ""
    for line in devices.splitlines():
        parts = line.split(":")
        if (
            len(parts) == 3
            and parts[1] == "wifi"
            and parts[2] == "connected"
            and re.fullmatch(r"[A-Za-z0-9_][A-Za-z0-9_.-]{0,63}", parts[0])
        ):
            interface = parts[0]
            break
    return {"enabled": enabled, "interface": interface}


def main():
    p = argparse.ArgumentParser()
    p.add_argument("action")
    p.add_argument("value", nargs="?", default="")
    a = p.parse_args()
    try:
        if a.action == "network-status":
            print(json.dumps(network_status()))
            return
        cmd = command(a.action, a.value)
        if a.action == "wifi-connect":
            subprocess.run(cmd, check=True)
            return 0
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=20)
        # bluetoothctl can return zero for an unsuccessful operation.
        output = result.stdout + result.stderr
        if result.returncode or any(
            word in output.lower() for word in ("failed", "not available", "not ready")
        ):
            raise RuntimeError(output.strip()[:400] or "Device action failed")
        message = (
            "Wi-Fi scan complete."
            if a.action == "wifi-scan"
            else output.strip()[:400] or "Device updated"
        )
        print(json.dumps(dict(message=message)))
    except (
        OSError,
        ValueError,
        KeyError,
        TypeError,
        RuntimeError,
        subprocess.SubprocessError,
    ) as e:
        if a.action == "wifi-connect":
            print("\nCould not connect to Wi-Fi. Check the network and credentials.")
            input("\nPress Enter to close.")
            return 1
        print(json.dumps(dict(error=str(e))))


if __name__ == "__main__":
    sys.exit(main())
