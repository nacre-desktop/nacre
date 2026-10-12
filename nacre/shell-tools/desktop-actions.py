#!/usr/bin/env python3
"""Desktop and AI launch commands, independent of the private knowledge browser."""

import argparse
import importlib.util
import json
from pathlib import Path
import re
import subprocess

import compositor

HOME = Path.home()
PROFILE_SPEC = importlib.util.spec_from_file_location(
    "desktop_profile_policy", Path(__file__).with_name("profile-policy.py")
)
POLICY = importlib.util.module_from_spec(PROFILE_SPEC)
PROFILE_SPEC.loader.exec_module(POLICY)


def run(command, default=None, timeout=10):
    result = subprocess.run(command, capture_output=True, text=True, timeout=timeout)
    if result.returncode:
        if default is not None:
            return default
        raise RuntimeError(result.stderr.strip() or "Desktop command failed")
    return result.stdout.strip()


def compositor_windows():
    try:
        return compositor.query("windows", timeout=10)
    except subprocess.CalledProcessError:
        return []


def compositor_action(name, *args, **options):
    try:
        return getattr(compositor, name)(*args, timeout=10, **options)
    except subprocess.CalledProcessError as error:
        raise RuntimeError(
            (error.stderr or "").strip() or "Desktop command failed"
        ) from error


def launch(command):
    subprocess.Popen(
        ["uwsm", "app", "--", *command],
        start_new_session=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


launch_app = launch


def action(name, value=""):
    actions = {
        "network": ["nm-connection-editor"],
        "bluetooth": ["blueman-manager"],
        "updates": [
            "kitty",
            "--class",
            "nacre-control",
            "--title",
            "System updates",
            "--",
            "bash",
            str(HOME / ".local/share/nacre/shell/tools/updates.sh"),
        ],
        "audio": ["pavucontrol"],
        "notifications": [str(HOME / ".local/bin/nacre-settings"), "notifications"],
        "play": ["playerctl", "play-pause"],
        "next": ["playerctl", "next"],
        "previous": ["playerctl", "previous"],
        "mute": ["wpctl", "set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"],
        "lock": ["loginctl", "lock-session"],
        "power": [str(HOME / ".local/bin/nacre-shell"), "session"],
        "files": [str(HOME / ".local/bin/nacre-app"), "files"],
        "terminal": [str(HOME / ".local/bin/nacre-app"), "terminal"],
        "tasks": [
            "kitty",
            "--class",
            "nacre-ai-dashboard",
            "--title",
            "Nacre AI",
            "--",
            "nacre-ai",
            "dashboard",
        ],
        "new": [
            "kitty",
            "--class",
            "nacre-ai-task",
            "--title",
            "New chat",
            "--",
            "nacre-ai",
            "window",
            "--worker-command",
            "new",
        ],
        "resume": [
            "kitty",
            "--class",
            "nacre-ai-task",
            "--title",
            "Resume task",
            "--",
            "nacre-ai",
            "window",
            "--worker-command",
            "latest",
        ],
    }
    if name in actions:
        if name == "tasks":
            clients = compositor_windows()
            old = next((c for c in clients if c["class"] == "nacre-ai-dashboard"), None)
            if old:
                return action("focus", old["address"])
        if name in ("new", "resume", "tasks"):
            workspace = POLICY.load(HOME)["aiWorkspace"]
            if workspace is not None:
                action("workspace", str(workspace))
        launch_app(actions[name])
        return
    if name == "volume":
        run(
            [
                "wpctl",
                "set-volume",
                "@DEFAULT_AUDIO_SINK@",
                str(max(0, min(100, int(value)))) + "%",
            ]
        )
        return
    if name == "brightness":
        run(
            [
                str(HOME / ".local/share/nacre/shell/bin/qs"),
                "-c",
                "nacre",
                "ipc",
                "call",
                "brightness",
                "set",
                str(max(0.01, min(1, int(value) / 100))),
            ]
        )
        return
    if name == "workspace":
        num = int(value)
        if num not in range(1, 11):
            raise ValueError("Invalid workspace")
        compositor_action("switch_workspace", num, current_monitor=True)
        return
    if name == "focus":
        if not compositor.valid_window_id(value):
            raise ValueError("Invalid window")
        compositor_action("focus_window", value)
        return
    if name == "app-windows":
        ids = json.loads(value)
        if not isinstance(ids, list) or not all(
            compositor.valid_window_id(i) for i in ids
        ):
            raise ValueError("Invalid window group")
        clients = compositor_windows()
        windows = sorted(
            (c for c in clients if c["address"] in ids),
            key=lambda c: (c["workspace"]["id"], c["title"]),
        )
        if len(windows) == 1:
            return action("focus", windows[0]["address"])
        if not windows:
            return
        labels = [
            str(c["workspace"]["id"]) + "  " + c["title"].replace("\n", " ")[:120]
            for c in windows
        ]
        try:
            choice = subprocess.check_output(
                ["nacre-menu", "pick", "--json", "--title", "Choose window"],
                input=json.dumps(
                    [
                        dict(label=label, value=str(index))
                        for index, label in enumerate(labels)
                    ]
                ),
                text=True,
                timeout=300,
            ).strip()
        except subprocess.SubprocessError:
            return
        if choice.isdigit() and int(choice) < len(windows):
            return action("focus", windows[int(choice)]["address"])
        return
    if name == "wifi":
        rows = []
        seen = set()
        for line in run(
            [
                "nmcli",
                "-t",
                "-f",
                "IN-USE,SSID,SIGNAL,SECURITY",
                "device",
                "wifi",
                "list",
                "--rescan",
                "no",
            ]
        ).splitlines():
            parts = re.split(r"(?<!\\):", line)
            if len(parts) < 4:
                continue
            active, ssid, signal, security = parts[:4]
            ssid = ssid.replace("\\:", ":").replace("\\\\", "\\")
            if not ssid or ssid in seen:
                continue
            seen.add(ssid)
            rows.append(
                (
                    ssid,
                    ("● " if active == "*" else "  ")
                    + ssid
                    + "   "
                    + signal
                    + "%"
                    + ("  " if security and security != "--" else ""),
                )
            )
        if not rows:
            launch(
                [
                    "kitty",
                    "--class",
                    "nacre-control",
                    "--title",
                    "Wi-Fi connections",
                    "--",
                    "nmtui-connect",
                ]
            )
            return
        try:
            choice = subprocess.check_output(
                ["nacre-menu", "pick", "--json", "--title", "Wi-Fi"],
                input=json.dumps(
                    [
                        dict(label=label, value=str(index))
                        for index, (_, label) in enumerate(rows)
                    ]
                ),
                text=True,
                timeout=300,
            ).strip()
        except subprocess.SubprocessError:
            return
        if choice.isdigit() and int(choice) < len(rows):
            # Native NetworkManager prompt owns credentials; never capture passwords.
            launch(
                [
                    "kitty",
                    "--class",
                    "nacre-control",
                    "--title",
                    "Connect Wi-Fi",
                    "--",
                    "nmcli",
                    "--ask",
                    "device",
                    "wifi",
                    "connect",
                    rows[int(choice)][0],
                ]
            )
        return
    raise ValueError("Unknown desktop action")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("action")
    parser.add_argument("value", nargs="?", default="")
    args = parser.parse_args()
    action(args.action, args.value)
