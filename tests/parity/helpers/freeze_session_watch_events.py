#!/usr/bin/env python3
"""Freeze existing resume/hotplug and connected-display policy, no real buses."""

import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "nacre/shell-tools/session-watch.py"
DEST = ROOT / "crates/nacre-helper/tests/fixtures/session-watch-policy-reference.json"


def load():
    spec = importlib.util.spec_from_file_location("original_session_policy", SOURCE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main():
    if DEST.exists():
        raise ValueError("Refusing to replace frozen session policy")
    restored = []
    previous = dict(os.environ)
    for name in ("niri", "missing", "disconnected", "connected"):
        with tempfile.TemporaryDirectory(prefix="session-display-policy-") as folder:
            try:
                os.environ.update(
                    HOME=folder,
                    XDG_CONFIG_HOME=folder + "/.config",
                    XDG_STATE_HOME=folder + "/.local/state",
                    XDG_DATA_HOME=folder + "/.local/share",
                    XDG_CACHE_HOME=folder + "/.cache",
                    DBUS_SESSION_BUS_ADDRESS="unix:path=" + folder + "/absent-session",
                    DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + folder + "/absent-system",
                    NACRE_COMPOSITOR="niri",
                )
                module = load()
                calls = []
                path = Path(folder) / ".config/nacre/desktop.json"
                saved = [
                    dict(name="DP-1", width=1920),
                    dict(name="HDMI-A-1", width=3840),
                ]
                if name != "missing":
                    path.parent.mkdir(parents=True)
                    path.write_text(json.dumps(dict(displays=saved)))
                module.compositor = SimpleNamespace(
                    capabilities=dict(monitorConfiguration=name != "niri"),
                    query=lambda *_, name=name, **__: (
                        [] if name == "disconnected" else [dict(name="DP-1")]
                    ),
                    configure_monitors=lambda value, calls=calls, **_: calls.append(
                        value
                    ),
                )
                # The historical function imports Settings without calling it;
                # retain that import, while replacing only hardware effects.
                module.restore_connected_displays()
                restored.append(dict(name=name, saved=saved, result=calls))
            finally:
                os.environ.clear()
                os.environ.update(previous)
    module = load()
    effects = []
    read_fd, write_fd = os.pipe()
    reader = os.fdopen(read_fd, "rb")

    class Connection:
        def __init__(self):
            self.read, self.write = os.pipe()

        def fileno(self):
            return self.read

        def close(self):
            effects.append(["close-events"])
            os.close(self.read)
            os.close(self.write)

    connection = Connection()

    class Observer:
        stdout = reader

        def terminate(self):
            effects.append(["unsubscribe-logind"])

    class Selector:
        def __init__(self):
            self.keys = {}
            self.index = 0

        def register(self, file, _, data):
            self.keys[data] = SimpleNamespace(fileobj=file, data=data)

        def select(self, **_):
            packets = [
                ("sleep", b"PrepareForSleep (true,)\n"),
                ("sleep", b"PrepareForSleep (false,)\n"),
                ("monitor", b"outputs\n"),
                ("sleep", None),
            ]
            kind, packet = packets[self.index]
            self.index += 1
            if packet is None:
                os.close(write_fd)
            elif kind == "sleep":
                os.write(write_fd, packet)
            else:
                os.write(connection.write, packet)
            return [(self.keys[kind], 1)]

    original_popen, original_selector, original_sleep = (
        module.subprocess.Popen,
        module.selectors.DefaultSelector,
        module.time.sleep,
    )
    try:
        module.check = lambda event: effects.append(["check", event])
        module.restore_connected_displays = lambda: effects.append(["restore"])
        module.subprocess.Popen = lambda *_a, **_k: Observer()
        module.selectors.DefaultSelector = Selector
        module.compositor = SimpleNamespace(
            event_socket=lambda: connection,
            parse_event=lambda _: dict(type="outputs-changed"),
        )
        module.time.sleep = lambda value: effects.append(["sleep", value])
        try:
            module.watch()
        except RuntimeError as error:
            effects.append(["error", str(error)])
    finally:
        module.subprocess.Popen, module.selectors.DefaultSelector, module.time.sleep = (
            original_popen,
            original_selector,
            original_sleep,
        )
        reader.close()
    DEST.write_text(
        json.dumps(
            dict(
                source_sha256=hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
                restore=restored,
                watch=effects,
            ),
            indent=2,
        )
        + "\n"
    )
    print(
        "Frozen",
        len(restored),
        "display policies and",
        len(effects),
        "ordered observer effects",
    )


if __name__ == "__main__":
    main()
