#!/usr/bin/env python3
"""Opt-in actual Super+Q test on fresh, private nested installations."""

import argparse
import contextlib
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "nacre/shell-tools"))
import nacre_native_config as compositor_config

spec = importlib.util.spec_from_file_location(
    "close_prepare", ROOT / "tools/prepare.py"
)
prepare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)


def build_keyboard(directory):
    url = "https://raw.githubusercontent.com/atx/wtype/d71be3a7b3f93b534a2823fd68cabd7ac2a02359/protocol/virtual-keyboard-unstable-v1.xml"
    with urllib.request.urlopen(url, timeout=20) as response:
        data = response.read()
    if (
        hashlib.sha256(data).hexdigest()
        != "7ad7870003ecd592cae47dc19d277a609b7f18fd7b7be012623cf3225a7294f5"
    ):
        raise ValueError("Virtual keyboard protocol checksum mismatch")
    xml = directory / "virtual-keyboard.xml"
    xml.write_bytes(data)
    for mode, suffix in (("client-header", "h"), ("private-code", "c")):
        subprocess.run(
            [
                "wayland-scanner",
                mode,
                str(xml),
                str(directory / ("virtual-keyboard." + suffix)),
            ],
            check=True,
        )
    flags = subprocess.check_output(
        ["pkg-config", "--cflags", "--libs", "wayland-client", "xkbcommon"], text=True
    ).split()
    binary = directory / "send-close"
    subprocess.run(
        [
            "cc",
            "-O2",
            "-I",
            str(directory),
            "-o",
            str(binary),
            str(ROOT / "tools/tests/plain-close-keyboard.c"),
            str(directory / "virtual-keyboard.c"),
            *flags,
        ],
        check=True,
    )
    return binary


def verify(backend, output, parent_binary, parent_libraries):
    processes = []
    with tempfile.TemporaryDirectory(prefix="nclose-") as temporary:
        home = Path(temporary) / "h"
        prepare.prepare(
            ROOT, home, json.loads((ROOT / "profiles/default.json").read_text())
        )
        runtime = home / "r"
        runtime.mkdir(mode=0o700)
        keyboard = build_keyboard(home)
        tools = home / ".local/share/nacre/shell/tools"
        env = dict(
            os.environ,
            HOME=str(home),
            XDG_RUNTIME_DIR=str(runtime),
            XDG_CONFIG_HOME=str(home / ".config"),
            XDG_DATA_HOME=str(home / ".local/share"),
            XDG_STATE_HOME=str(home / ".local/state"),
            XDG_CACHE_HOME=str(home / ".cache"),
            NACRE_COMPOSITOR=backend,
            NACRE_COMPOSITOR_TOOLS=str(tools),
            DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + str(home / "no-system-bus"),
        )
        for key in (
            "NIRI_SOCKET",
            "HYPRLAND_INSTANCE_SIGNATURE",
            "DISPLAY",
            "NIRI_CONFIG",
            "NACRE_NIRI_CONFIG",
            "NACRE_NIRI_DESKTOP_CONFIG",
            "NACRE_PROFILE_FILE",
            "WAYLAND_SOCKET",
        ):
            env.pop(key, None)
        log_path = output.with_suffix(".log")
        with log_path.open("w") as log:

            def start(command, process_env=None, **kwargs):
                process = subprocess.Popen(
                    command,
                    env=process_env or env,
                    stderr=log,
                    stdout=kwargs.pop("stdout", log),
                    start_new_session=True,
                    **kwargs,
                )
                processes.append(process)
                return process

            def run(command):
                return subprocess.run(
                    command,
                    env=env,
                    capture_output=True,
                    text=True,
                    check=True,
                    timeout=10,
                )

            def query(name):
                return json.loads(
                    run(["python3", str(tools / "compositor.py"), "query", name]).stdout
                )

            def wait_for(callback):
                deadline = time.monotonic() + 15
                while time.monotonic() < deadline:
                    value = callback()
                    if value:
                        return value
                    time.sleep(0.1)
                raise RuntimeError("Nested close probe timed out")

            try:
                bus_config = home / "bus.conf"
                bus_config.write_text(
                    '<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>'
                )
                bus = start(
                    [
                        "dbus-daemon",
                        "--nofork",
                        "--config-file=" + str(bus_config),
                        "--print-address=1",
                    ],
                    stdout=subprocess.PIPE,
                    text=True,
                )
                env["DBUS_SESSION_BUS_ADDRESS"] = bus.stdout.readline().strip()
                if not env["DBUS_SESSION_BUS_ADDRESS"]:
                    raise RuntimeError("Private bus failed")
                parent_config = home / "parent"
                parent_config.mkdir()
                parent_config.joinpath("rc.xml").write_text(
                    '<labwc_config><keyboard><repeatRate>25</repeatRate></keyboard><windowRules><windowRule identifier="niri"><action name="ToggleFullscreen"/></windowRule><windowRule identifier="Hyprland"><action name="ToggleFullscreen"/></windowRule></windowRules></labwc_config>'
                )
                parent_env = dict(
                    env,
                    WLR_BACKENDS="headless",
                    WLR_RENDERER="gles2",
                    WLR_RENDERER_ALLOW_SOFTWARE="1",
                    WLR_LIBINPUT_NO_DEVICES="1",
                    WLR_HEADLESS_OUTPUTS="1",
                )
                parent_env.pop("WAYLAND_DISPLAY", None)
                parent_env.pop("DISPLAY", None)
                if parent_libraries:
                    parent_env["LD_LIBRARY_PATH"] = str(parent_libraries)
                start(
                    [str(parent_binary), "-C", str(parent_config)],
                    process_env=parent_env,
                )
                parent = wait_for(
                    lambda: next(
                        (
                            path
                            for path in runtime.glob("wayland-*")
                            if path.is_socket()
                        ),
                        None,
                    )
                )
                env["WAYLAND_DISPLAY"] = str(parent)
                if backend == "niri":
                    # Install complete generated fragments from the staged contract.
                    directory = home / ".config/nacre/niri"
                    directory.mkdir(parents=True)
                    for name, text in compositor_config.render(
                        compositor_config.load(tools), backend="niri"
                    ).items():
                        (directory / Path(name).name).write_text(text)
                    config = directory / "config.kdl"
                    config.write_text(
                        'include "bindings.kdl"\ninclude "window-rules.kdl"\nhotkey-overlay { skip-at-startup; }\n'
                    )
                    env["NACRE_NIRI_CONFIG"] = str(config)
                    compositor = start(["/usr/bin/niri", "-c", str(config)])
                    socket = wait_for(
                        lambda: next(
                            iter(runtime.glob(f"niri.*.{compositor.pid}.sock")), None
                        )
                    )
                    env["NIRI_SOCKET"] = str(socket)
                    binding = "Super+Q"
                else:
                    config = home / "close.lua"
                    config.write_text(
                        'hl.monitor({output="",mode="1280x800@60",position="auto",scale=1})\nhl.config({animations={enabled=false},misc={disable_hyprland_logo=true,disable_splash_rendering=true}})\ndofile(os.getenv("HOME") .. "/.config/hypr/conf/keybinding.lua")\n'
                    )
                    env["AQ_BACKEND"] = "wayland"
                    start(["Hyprland", "-c", str(config)])
                    socket = wait_for(
                        lambda: next(iter(runtime.glob("hypr/*/.socket.sock")), None)
                    )
                    env["HYPRLAND_INSTANCE_SIGNATURE"] = socket.parent.name
                    run(["hyprctl", "output", "create", "headless"])
                    errors = run(["hyprctl", "configerrors"]).stdout.strip()
                    if errors and errors != "ok":
                        raise RuntimeError(
                            "Staged Hyprland config has errors: " + errors[:500]
                        )
                    bindings = json.loads(run(["hyprctl", "-j", "binds"]).stdout)
                    assert any(
                        row["key"] == "Q"
                        and row["modmask"] == 64
                        and row.get("description") == "Nacre:close"
                        for row in bindings
                    )
                    binding = "SUPER + Q"
                display = wait_for(
                    lambda: next(
                        (
                            path
                            for path in runtime.glob("wayland-*")
                            if path.is_socket() and path != parent
                        ),
                        None,
                    )
                )
                env["WAYLAND_DISPLAY"] = str(display)
                client = start(
                    [
                        "kitty",
                        "--config",
                        "NONE",
                        "-o",
                        "confirm_os_window_close=0",
                        "--title",
                        "Close Probe",
                        "sh",
                        "-c",
                        "sleep 180",
                    ]
                )
                window = wait_for(
                    lambda: next(
                        (
                            row
                            for row in query("windows")
                            if row["title"] == "Close Probe"
                        ),
                        None,
                    )
                )
                identifier = window["windowId"]
                run(
                    [
                        "python3",
                        str(tools / "compositor.py"),
                        "action",
                        "focus_window",
                        "--window",
                        identifier,
                    ]
                )
                wait_for(lambda: query("active_window").get("windowId") == identifier)
                # Niri needs parent-forwarded input; Hyprland accepts child input.
                child_display = env["WAYLAND_DISPLAY"]
                if backend == "niri":
                    env["WAYLAND_DISPLAY"] = str(parent)
                run([str(keyboard)])
                env["WAYLAND_DISPLAY"] = child_display
                wait_for(
                    lambda: (
                        not any(
                            row["windowId"] == identifier for row in query("windows")
                        )
                    )
                )
                client.wait(timeout=5)
                result = {
                    "backend": backend,
                    "status": "passed",
                    "freshDefaultInstall": True,
                    "binding": binding,
                    "input": "private headless parent keyboard"
                    if backend == "niri"
                    else "child-socket virtual keyboard",
                    "windowDisappeared": True,
                    "clientExited": True,
                }
                output.write_text(json.dumps(result, indent=2) + "\n")
                print(json.dumps(result))
            finally:
                for process in reversed(processes):
                    if process.poll() is None:
                        with contextlib.suppress(ProcessLookupError):
                            os.killpg(process.pid, signal.SIGTERM)
                for process in processes:
                    with contextlib.suppress(subprocess.TimeoutExpired):
                        process.wait(timeout=3)
                    if process.poll() is None:
                        with contextlib.suppress(ProcessLookupError):
                            os.killpg(process.pid, signal.SIGKILL)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backend", required=True, choices=("hyprland", "niri"))
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--parent", default="labwc", type=Path)
    parser.add_argument("--parent-libraries", type=Path)
    args = parser.parse_args()
    verify(args.backend, args.output.resolve(), args.parent, args.parent_libraries)
