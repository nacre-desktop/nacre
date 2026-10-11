#!/usr/bin/env python3
"""Explicit capture/night-light actions and read-only optional tool discovery."""

import argparse
import json
import os
from pathlib import Path
import shutil
import socket
import time
import subprocess
import tempfile


def record_path(home=None):
    return (home or Path.home()) / ".local/state/nacre/control-tools.json"


def service():
    result = subprocess.run(
        [
            "systemctl",
            "--user",
            "show",
            "hyprsunset.service",
            "-p",
            "ActiveState",
            "-p",
            "MainPID",
            "-p",
            "InvocationID",
        ],
        capture_output=True,
        text=True,
        timeout=5,
    )
    fields = dict(
        line.split("=", 1) for line in result.stdout.splitlines() if "=" in line
    )
    active = fields.get("ActiveState") == "active"
    pid = fields.get("MainPID", "0")
    invocation = fields.get("InvocationID", "")
    owner = pid + ":" + invocation if pid != "0" and invocation else ""
    return active, owner


def load(home=None):
    try:
        data = json.loads(record_path(home).read_text())
        return data if isinstance(data, dict) else {}
    except (OSError, ValueError):
        return {}


def save(data, home=None):
    path = record_path(home)
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(dir=path.parent)
    try:
        with os.fdopen(fd, "w") as stream:
            json.dump(data, stream)
        os.chmod(temporary, 0o600)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def configured_elsewhere(home=None):
    home = home or Path.home()
    return (home / ".config/hypr/hyprsunset.conf").exists()


def unmanaged_socket(active):
    signature = os.environ.get("HYPRLAND_INSTANCE_SIGNATURE", "")
    runtime = os.environ.get("XDG_RUNTIME_DIR", "")
    if active or not signature or not runtime:
        return False
    path = Path(runtime) / "hypr" / signature / ".hyprsunset.sock"
    if not path.exists():
        return False
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
            connection.settimeout(0.15)
            connection.connect(str(path))
        return True
    except (ConnectionRefusedError, FileNotFoundError):
        return False
    except OSError:
        # Ambiguous or inaccessible listeners retain external ownership.
        return True


def apply_temperature(command, owner, starting):
    for attempt in range(12 if starting else 1):
        try:
            subprocess.run(command, check=True, capture_output=True, timeout=1)
            return
        except subprocess.SubprocessError:
            if not starting or attempt == 11 or service()[1] != owner:
                raise
            time.sleep(0.05)


def state(home=None):
    available = bool(shutil.which("hyprsunset"))
    active, owner = service() if available else (False, "")
    data = load(home)
    owned = active and bool(owner) and data.get("owner") == owner
    return {
        "screenshot": bool(shutil.which("grim") and shutil.which("slurp")),
        "colorPicker": bool(shutil.which("hyprpicker")),
        "powerProfilesSupported": bool(shutil.which("powerprofilesctl")),
        "nightLightSupported": available,
        "nightLightExternal": (active and not owned)
        or configured_elsewhere(home)
        or unmanaged_socket(active),
        "nightLightEnabled": owned and data.get("enabled") is True,
    }


def night_light(enabled, home=None):
    if not shutil.which("hyprsunset"):
        raise ValueError("Install the system hyprsunset package for Night light")
    active, owner = service()
    data = load(home)
    if (
        configured_elsewhere(home)
        or unmanaged_socket(active)
        or (active and (not owner or data.get("owner") != owner))
    ):
        raise ValueError(
            "Night light is managed externally; use its existing controller"
        )
    started = False
    if enabled and not active:
        subprocess.run(
            ["systemctl", "--user", "start", "hyprsunset.service"],
            check=True,
            capture_output=True,
            timeout=10,
        )
        active, owner = service()
        if not active or not owner:
            raise RuntimeError("Night-light service did not become ready")
        started = True
        # Claim only the service this action started; never another running owner.
        save({"owner": owner, "enabled": False}, home)
    if active:
        command = (
            ["hyprctl", "hyprsunset", "temperature", "4500"]
            if enabled
            else ["hyprctl", "hyprsunset", "identity"]
        )
        try:
            apply_temperature(command, owner, started)
        except subprocess.SubprocessError:
            # Undo a partial startup, but only while the invocation still belongs to us.
            if started and service()[1] == owner:
                subprocess.run(
                    ["systemctl", "--user", "stop", "hyprsunset.service"],
                    check=True,
                    capture_output=True,
                    timeout=10,
                )
            raise
    save({"owner": owner if active else "", "enabled": bool(enabled and active)}, home)
    return state(home)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "action", choices=["state", "night-light", "screenshot", "color-picker"]
    )
    parser.add_argument("value", nargs="?", choices=["on", "off"])
    args = parser.parse_args()
    try:
        if args.action == "state":
            print(json.dumps(state()))
        elif args.action == "night-light":
            if args.value is None:
                raise ValueError("Choose on or off")
            print(json.dumps(night_light(args.value == "on")))
        else:
            command = (
                ["python3", str(Path(__file__).with_name("screenshot.py")), "region"]
                if args.action == "screenshot"
                else ["hyprpicker", "-a"]
            )
            subprocess.run(command, check=True, timeout=180)
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print(json.dumps({"error": str(error)[:300]}))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
