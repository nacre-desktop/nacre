"""Developer-only callbacks; all settings decisions execute in the Rust port."""

import importlib.util
import json
import os
from pathlib import Path
import select
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import nacre_native_compositor as compositor
from nacre_native_palette import classic_state as palette

REPO = Path(__file__).resolve().parents[3]
HOME = Path.home()
STATE = HOME / ".config/nacre/desktop.json"
LUA = compositor.config_path(HOME, "desktop")
PENDING = STATE.with_name(
    "display-pending-niri.json"
    if compositor.backend_name == "niri"
    else "display-pending.json"
)


def publish_mono_font(family):
    from nacre_native_config import _binary

    subprocess.run([str(_binary()), "terminal-font", family], check=True)


def monitor_state():
    return invoke("monitors")


_ORIGINAL_MONITORS = monitor_state


def handle(event):
    kind = event["kind"]
    if kind == "compositor":
        operation = event["operation"]
        value = getattr(compositor, operation)(*event["args"], **event["kwargs"])
        return (
            None
            if operation
            in {
                "set_desktop_options",
                "configure_monitors",
                "ensure_desktop_loader",
                "apply_config",
                "reload",
            }
            else value
        )
    if kind == "monitor_state":
        return monitor_state()
    if kind == "write":
        return palette.atomic_write(Path(event["path"]), event["text"])
    if kind == "font":
        publish_mono_font(event["family"])
        return None
    if kind == "workflow":
        spec = importlib.util.spec_from_file_location(
            "test_workflows", REPO / "nacre/shell-tools/workflow-profiles.py"
        )
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        module.HOME = HOME
        module.PATH = HOME / ".config/nacre/workflows.json"
        return (
            module.apply(event["name"])
            if event["action"] == "apply"
            else module.save(event["name"], event["roles"])
        )
    if kind == "sleep":
        return time.sleep(event["seconds"])
    raise AssertionError("Unscripted native settings test callback: " + kind)


def invoke(operation, **values):
    binary = os.environ.get(
        "NACRE_DESKTOP_TEST", str(REPO / "target/debug/examples/desktop-settings-test")
    )
    request = dict(operation=operation, **values)
    if operation not in {"metadata", "validate", "display_plan"}:
        request.update(
            home=str(HOME),
            state=str(STATE),
            config=str(LUA),
            pending=str(PENDING),
            defaults=DEFAULTS,
            backend=compositor.backend_name,
            capabilities=dict(compositor.capabilities),
            options=OPTIONS,
            monitor_override=monitor_state is not _ORIGINAL_MONITORS,
        )
    with subprocess.Popen(
        [binary],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    ) as process:
        process.stdin.write(json.dumps(request) + "\n")
        process.stdin.flush()
        while True:
            if not select.select([process.stdout], [], [], 15)[0]:
                process.kill()
                raise RuntimeError("Native settings test provider timed out")
            line = process.stdout.readline()
            if not line:
                raise RuntimeError(
                    "Native settings test provider exited: "
                    + process.stderr.read()[-1000:]
                )
            packet = json.loads(line)
            if "print" in packet:
                print(packet["print"])
                continue
            if "callback" not in packet:
                process.wait(timeout=5)
                if "error" in packet:
                    raise ValueError(packet["error"])
                return packet["result"]
            try:
                value = handle(packet["callback"])
                response = dict(result=value)
            except Exception as error:  # noqa: BLE001 — Forward the injected callback failure to Rust, then retain its original assertions.
                response = dict(error=str(error))
            process.stdin.write(json.dumps(response) + "\n")
            process.stdin.flush()


_METADATA = invoke("metadata")
DEFAULTS = _METADATA["defaults"]
INPUT_KEYS = _METADATA["inputKeys"]
PRESET_KEYS = _METADATA["presetKeys"]
OPTIONS = compositor.desktop_option_keys()


def validate(key, value):
    return invoke(
        "validate",
        key=key,
        value=value,
        developer=os.environ.get("NACRE_DEVELOPER_MODE") == "1",
    )


def display_plan(monitors, primary, mode):
    return invoke("display_plan", monitors=monitors, primary=primary, mode=mode)


def load():
    return invoke("load")


def persist(data):
    return invoke("persist", data=data)


def revert(token=None):
    return invoke("revert", token=token)


def main():
    return invoke("main", args=sys.argv[1:])
