#!/usr/bin/env python3
"""Readiness and non-disruptive login/resume/hotplug evidence; no secret values."""

import nacre_native_compositor as compositor

import argparse
from nacre_i18n import tr
import datetime as dt
import json
import os
import selectors
import subprocess
import time
from pathlib import Path

HOME = Path.home()
STATE = HOME / ".local/state/nacre/session"


def run(args, default="", timeout=5):
    try:
        return subprocess.check_output(
            args, text=True, stderr=subprocess.DEVNULL, timeout=timeout
        ).strip()
    except (OSError, subprocess.SubprocessError):
        return default


def check(event="manual"):
    STATE.mkdir(parents=True, exist_ok=True, mode=0o700)
    qs = HOME / ".local/share/nacre/shell/bin/qs"
    desktop = run([str(qs), "-c", "nacre", "ipc", "call", "nacre", "state"])
    try:
        desktopReady = "launcherMode" in json.loads(desktop)
    except ValueError:
        desktopReady = False
    wallet = run(
        [
            "busctl",
            "--user",
            "get-property",
            "org.freedesktop.secrets",
            "/org/freedesktop/secrets/aliases/default",
            "org.freedesktop.Secret.Collection",
            "Locked",
        ]
    )
    monitors = compositor.query("monitors", timeout=5, default=[])
    clients = compositor.query("windows", timeout=5, default=[])
    record = dict(
        event=event,
        checked=dt.datetime.now(dt.timezone.utc).isoformat(),
        desktopReady=desktopReady,
        wallet="unlocked"
        if wallet == "b false"
        else "locked"
        if wallet == "b true"
        else "unavailable",
        monitors=[
            dict(name=m["name"], width=m["width"], height=m["height"], scale=m["scale"])
            for m in monitors
        ],
        brainWindows=sum(
            c.get("class") in ("nacre-brain", "chrome-127.0.0.1__-Default")
            for c in clients
        ),
        configErrors=compositor.config_errors(timeout=5, default=""),
    )
    temp = STATE / "latest.next"
    temp.write_text(json.dumps(record))
    temp.chmod(0o600)
    os.replace(temp, STATE / "latest.json")
    with (STATE / "events.jsonl").open("a") as stream:
        stream.write(json.dumps(record) + "\n")
    (STATE / "events.jsonl").chmod(0o600)
    return record


def restore_connected_displays():
    if compositor.capabilities.get("monitorConfiguration") is False:
        return  # Niri restores outputs from its own configuration.
    config = HOME / ".config/nacre/desktop.json"
    if not config.exists():
        return
    data = json.loads(config.read_text())
    monitors = compositor.query("monitors", timeout=5, default=[])
    connected = {m["name"] for m in monitors}
    saved = [m for m in data.get("displays", []) if m["name"] in connected]
    if saved:
        import importlib.util

        spec = importlib.util.spec_from_file_location(
            "settings", Path(__file__).with_name("desktop-settings.py")
        )
        m = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(m)
        compositor.configure_monitors(saved, timeout=None)


def readiness(timeout=15):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = check("startup-readiness")
        if value["desktopReady"] and value["wallet"] in ("unlocked", "unavailable"):
            return value
        time.sleep(0.5)
    return check("startup-timeout")


def watch():
    # login1 records actual resume, not a simulated success. The compositor socket records hotplug.
    check("observer-start")
    restore_connected_displays()
    observer = subprocess.Popen(
        [
            "gdbus",
            "monitor",
            "--system",
            "--dest",
            "org.freedesktop.login1",
            "--object-path",
            "/org/freedesktop/login1",
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )
    connection = compositor.event_socket()
    selector = selectors.DefaultSelector()
    selector.register(observer.stdout, selectors.EVENT_READ, "sleep")
    selector.register(connection, selectors.EVENT_READ, "monitor")
    buffers = {"sleep": b"", "monitor": b""}
    try:
        while True:
            for key, _ in selector.select(timeout=30):
                chunk = os.read(key.fileobj.fileno(), 65536)
                if not chunk:
                    raise RuntimeError(
                        tr(
                            "helper.session-watch.session-observer-ended",
                            "Session observer ended",
                        )
                    )
                buffers[key.data] += chunk
                while b"\n" in buffers[key.data]:
                    line, buffers[key.data] = buffers[key.data].split(b"\n", 1)
                    text = line.decode(errors="replace")
                    if key.data == "sleep" and "PrepareForSleep" in text:
                        if "false" in text:
                            time.sleep(0.5)
                            restore_connected_displays()
                            check("resume")
                        elif "true" in text:
                            check("before-sleep")
                    elif (
                        key.data == "monitor"
                        and compositor.parse_event(text)["type"] == "outputs-changed"
                    ):
                        time.sleep(0.3)
                        restore_connected_displays()
                        check("display-hotplug")
    finally:
        observer.terminate()
        connection.close()


def main():
    p = argparse.ArgumentParser()
    p.add_argument("action", choices=["check", "ready", "watch"])
    p.add_argument("--event", default="manual")
    a = p.parse_args()
    if a.action == "watch":
        watch()
    else:
        print(json.dumps(readiness() if a.action == "ready" else check(a.event)))


if __name__ == "__main__":
    main()
