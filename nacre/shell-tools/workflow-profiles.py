#!/usr/bin/env python3
"""Saved personal audio/display/startup intent, applied only on explicit preset use."""

import json
import nacre_state as _state
from nacre_i18n import tr
import subprocess
from pathlib import Path

import nacre_native_compositor as compositor

HOME = Path.home()
PATH = HOME / ".config/nacre/workflows.json"


def run(args):
    return subprocess.check_output(args, text=True, stderr=subprocess.DEVNULL).strip()


def load():
    return _state.read_json_file(PATH) if PATH.exists() else {}


def devices():
    result = {}
    for kind in ("sinks", "sources"):
        try:
            result[kind] = [
                dict(name=row["name"], description=row.get("description", row["name"]))
                for row in json.loads(run(["pactl", "--format=json", "list", kind]))
            ]
        except (OSError, subprocess.SubprocessError, ValueError):
            result[kind] = []
    return result


def save(name, roles):
    if name not in (
        "normal",
        "focused",
        "presentation",
        "minimal",
        "meeting",
        "music",
        "docked",
    ):
        raise ValueError(
            tr(
                "helper.workflow-profiles.unknown-workflow-preset",
                "Unknown workflow preset",
            )
        )
    if not isinstance(roles, list) or any(
        r not in ("Browser", "Nacre AI", "Discord", "Spotify", "Mail", "Brain")
        for r in roles
    ):
        raise ValueError(
            tr(
                "helper.workflow-profiles.invalid-startup-roles",
                "Invalid startup roles",
            )
        )
    profiles = load()
    profiles[name] = dict(
        sink=run(["pactl", "get-default-sink"]),
        source=run(["pactl", "get-default-source"]),
        displays=compositor.query("monitors", timeout=None),
        roles=roles,
    )
    PATH.parent.mkdir(parents=True, exist_ok=True)
    _state.write(PATH, profiles)
    PATH.chmod(0o600)


def apply(name):
    profile = load().get(name)
    if not profile:
        return
    available = devices()
    for key, kind in (("sink", "sinks"), ("source", "sources")):
        if profile.get(key) in {d["name"] for d in available[kind]}:
            subprocess.run(["pactl", "set-default-" + key, profile[key]], check=True)
    # Preserve saved layouts only for currently connected outputs; do not disable or invent displays.
    current = {m["name"] for m in compositor.query("monitors", timeout=None)}
    saved = [m for m in profile.get("displays", []) if m["name"] in current]
    if saved:
        compositor.configure_monitors(saved, timeout=None)
    # Apps are opt-in; matching windows prevent duplicates and working chats are not closed.
    if profile.get("roles"):
        import importlib.util

        spec = importlib.util.spec_from_file_location(
            "startup", Path(__file__).with_name("startup-apps.py")
        )
        m = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(m)
        clients = compositor.query("windows", timeout=None)
        for label, _workspace, argv, exists in m.plan(clients):
            if label in profile["roles"] and not exists:
                subprocess.Popen(
                    ["systemd-run", "--user", "--scope", "--collect", "--quiet", *argv],
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    start_new_session=True,
                )


# Temporary internal bridge for the earlier desktop-settings port. The library's
# existing API remains canonical until its own Wave 4 port deletes this file.
if __name__ == "__main__":
    import sys

    if sys.argv[1:] == ["--native-controller-bridge"]:
        try:
            raw = sys.stdin.buffer.read(1024 * 1024 + 1)
            if len(raw) > 1024 * 1024:
                raise ValueError("Workflow request is too large")
            request = json.loads(raw)
            if not isinstance(request, dict):
                raise ValueError("Expected a workflow request object")
            if request.get("action") == "apply":
                apply(request["name"])
            elif request.get("action") == "save":
                save(request["name"], request["roles"])
            else:
                raise ValueError("Unknown workflow action")
            print(json.dumps({"ok": True}))
        except Exception as error:  # noqa: BLE001 — Return the original library failure to the native settings error UI.
            print(json.dumps({"error": str(error)}))
            sys.exit(1)
