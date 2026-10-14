#!/usr/bin/env python3
"""Exercise a prepared default installation through real compositor and shell IPC."""

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]


def wait_for(description, probe, processes, timeout=45):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        for process in processes:
            if process.poll() is not None:
                raise RuntimeError(
                    description + ": child exited; inspect the smoke logs"
                )
        result = probe()
        if result:
            return result
        time.sleep(0.05)
    raise TimeoutError("Timed out waiting for " + description)


def stop(process):
    if process.poll() is None:
        os.killpg(process.pid, signal.SIGTERM)
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=5)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--compositor", choices=("niri", "hyprland"), default="niri")
    parser.add_argument("--allow-unavailable-compositor", action="store_true")
    parser.add_argument("--quickshell", default=shutil.which("qs") or "quickshell")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    with tempfile.TemporaryDirectory(prefix="ns-") as folder:
        base = Path(folder)
        home, staging, runtime = (
            base / "installer-home",
            base / "staging",
            base / "runtime",
        )
        home.mkdir()
        staging.mkdir()
        runtime.mkdir(mode=0o700)
        env = {
            key: value
            for key, value in os.environ.items()
            if key
            in ("PATH", "LANG", "LC_ALL", "LD_LIBRARY_PATH", "NACRE_ORIENT_BINARY")
        }
        env.update(HOME=str(home), XDG_RUNTIME_DIR=str(runtime), NACRE_LANGUAGE="en")
        with (output / "prepare.log").open("w") as log:
            subprocess.run(
                [
                    str(ROOT / "install.sh"),
                    "--profile",
                    "default",
                    "--prepare",
                    str(staging),
                ],
                cwd=ROOT,
                env=env,
                stdin=subprocess.DEVNULL,
                stdout=log,
                stderr=subprocess.STDOUT,
                check=True,
                timeout=60,
            )
        if any(home.iterdir()):
            raise RuntimeError("Preparation wrote into the empty installer HOME")
        shell_dir = staging / ".local/share/nacre/shell"
        source = shell_dir / "source/shell.qml"
        if not source.is_file():
            raise RuntimeError("Prepared installation lacks the shell entry point")
        # --prepare intentionally installs no dependencies. Supply the candidate
        # test runtime as an installation prerequisite, outside the source tree.
        link = staging / ".local/share/nacre/palette-runtime/venv"
        link.parent.mkdir(parents=True)
        link.symlink_to(Path(sys.prefix))
        bridge = shell_dir / "bin/nacre_shell"
        bridge.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(shell_dir / "tools/cli-bridge.sh", bridge)
        bridge.chmod(0o755)
        env.update(
            HOME=str(staging),
            XDG_CONFIG_HOME=str(staging / ".config"),
            XDG_DATA_HOME=str(staging / ".local/share"),
            XDG_CACHE_HOME=str(staging / ".cache"),
            XDG_STATE_HOME=str(staging / ".local/state"),
            XDG_CURRENT_DESKTOP=args.compositor,
            XDG_SESSION_TYPE="wayland",
            NACRE_COMPOSITOR=args.compositor,
            QT_QPA_PLATFORM="wayland",
            LIBGL_ALWAYS_SOFTWARE="1",
            WLR_RENDERER_ALLOW_SOFTWARE="1",
        )
        with (output / "fonts.log").open("w") as log:
            subprocess.run(
                [sys.executable, str(shell_dir / "tools/font-setup.py"), "--apply"],
                env=env,
                stdin=subprocess.DEVNULL,
                stdout=log,
                stderr=subprocess.STDOUT,
                check=True,
                timeout=90,
            )
        children = []
        logs = []

        def spawn(name, command, child_env=env):
            log = (output / (name + ".log")).open("w")
            logs.append(log)
            process = subprocess.Popen(
                command,
                cwd=ROOT,
                env=child_env,
                stdin=subprocess.DEVNULL,
                stdout=log,
                stderr=subprocess.STDOUT,
                start_new_session=True,
            )
            children.append(process)
            return process

        try:
            # Own a private bus and X display; no desktop session is inherited.
            bus = spawn(
                "dbus", ["dbus-daemon", "--session", "--nofork", "--print-address=1"]
            )

            def bus_address():
                text = (output / "dbus.log").read_text()
                return next(
                    (line for line in text.splitlines() if line.startswith("unix:")),
                    None,
                )

            env["DBUS_SESSION_BUS_ADDRESS"] = wait_for(
                "private D-Bus", bus_address, [bus]
            )
            if args.compositor == "niri":
                read_fd, write_fd = os.pipe()
                log = (output / "xvfb.log").open("w")
                logs.append(log)
                xvfb = subprocess.Popen(
                    [
                        "Xvfb",
                        "-displayfd",
                        str(write_fd),
                        "-screen",
                        "0",
                        "1600x1000x24",
                        "-nolisten",
                        "tcp",
                        "-ac",
                    ],
                    env=env,
                    stdin=subprocess.DEVNULL,
                    stdout=log,
                    stderr=subprocess.STDOUT,
                    pass_fds=(write_fd,),
                    start_new_session=True,
                )
                children.append(xvfb)
                os.close(write_fd)
                os.set_blocking(read_fd, False)
                display = bytearray()

                def x_display():
                    try:
                        display.extend(os.read(read_fd, 64))
                    except BlockingIOError:
                        pass
                    return display.decode().strip() if b"\n" in display else None

                env["DISPLAY"] = ":" + wait_for("private X display", x_display, [xvfb])
                os.close(read_fd)
                config = base / "niri.kdl"
                config.write_text(
                    "input {\n keyboard {\n  xkb {}\n }\n}\nprefer-no-csd\nhotkey-overlay {\n skip-at-startup\n}\n"
                )
                subprocess.run(
                    ["niri", "validate", "--config", str(config)],
                    env=env,
                    check=True,
                    capture_output=True,
                    timeout=10,
                )
                compositor = spawn("compositor", ["niri", "--config", str(config)])
            else:
                env.pop("DISPLAY", None)
                env.update(AQ_BACKEND="headless", AQ_NO_KMS_REQUIREMENT="1")
                config = base / "hyprland.conf"
                config.write_text(
                    "monitor = ,1600x1000@60,0x0,1\nmisc {\n disable_hyprland_logo = true\n}\n"
                )
                compositor = spawn("compositor", ["Hyprland", "--config", str(config)])
            try:
                display_path = wait_for(
                    "nested Wayland socket",
                    lambda: next(
                        (p for p in runtime.glob("wayland-*") if p.is_socket()), None
                    ),
                    [compositor],
                )
            except (RuntimeError, TimeoutError):
                if (
                    args.compositor != "hyprland"
                    or not args.allow_unavailable_compositor
                ):
                    raise
                (output / "result.json").write_text(
                    json.dumps(
                        {
                            "compositor": "hyprland",
                            "status": "unavailable",
                            "reason": "Headless backend could not start; see compositor.log",
                        },
                        indent=2,
                    )
                    + "\n"
                )
                print(
                    "Hyprland headless backend unavailable; niri smoke remains required",
                    flush=True,
                )
                return
            env["WAYLAND_DISPLAY"] = display_path.name
            if args.compositor == "niri":
                socket = wait_for(
                    "niri IPC",
                    lambda: next(runtime.glob("niri*.sock"), None),
                    [compositor],
                )
                env["NIRI_SOCKET"] = str(socket)
            else:
                instance = wait_for(
                    "Hyprland IPC",
                    lambda: next((runtime / "hypr").glob("*"), None),
                    [compositor],
                )
                env["HYPRLAND_INSTANCE_SIGNATURE"] = instance.name
            shell = spawn("shell", [args.quickshell, "--path", str(source)])

            def ipc(target, method, *arguments):
                result = subprocess.run(
                    [
                        args.quickshell,
                        "ipc",
                        "--pid",
                        str(shell.pid),
                        "call",
                        target,
                        method,
                        *arguments,
                    ],
                    env=env,
                    stdin=subprocess.DEVNULL,
                    capture_output=True,
                    text=True,
                    timeout=10,
                )
                return result.stdout.strip() if result.returncode == 0 else None

            snapshots = {}

            def state(target="nacre"):
                raw = ipc(target, "state")
                try:
                    data = json.loads(raw) if raw else None
                    if data:
                        snapshots[target] = data
                    return data
                except ValueError:
                    return None

            def matches(target, predicate):
                data = state(target)
                return data if data and predicate(data) else None

            watch = [compositor, shell]
            wait_for("shell IPC", state, watch)
            print("Prepared shell started; exercising IPC", flush=True)
            ipc("nacre", "launcher", "smoke-probe")
            wait_for(
                "launcher",
                lambda: matches(
                    "nacre",
                    lambda row: (
                        row.get("launcher") and row.get("query") == "smoke-probe"
                    ),
                ),
                watch,
            )
            ipc("nacre", "tab", "1")
            wait_for(
                "dashboard",
                lambda: matches(
                    "nacre", lambda row: row.get("dashboard") and row.get("tab") == 1
                ),
                watch,
            )
            ipc("nacre", "settings")
            wait_for(
                "settings",
                lambda: matches(
                    "settingsApp", lambda row: row.get("visible") or row.get("active")
                ),
                watch,
            )
            picture = shell_dir / "tools/demo-wallpapers/forest.jpg"
            if not picture.is_file():
                raise RuntimeError(
                    "Prepared installation lacks its public demo wallpaper"
                )
            ipc("wallpaper", "set", str(picture))
            try:
                wait_for(
                    "wallpaper selection",
                    lambda: matches(
                        "wallpaper",
                        lambda row: (
                            row.get("applied") == str(picture) and not row.get("busy")
                        ),
                    ),
                    watch,
                    timeout=90,
                )
            except (RuntimeError, TimeoutError):
                diagnostic = subprocess.run(
                    [str(bridge), "wallpaper", "--file", str(picture)],
                    env=env,
                    capture_output=True,
                    text=True,
                    timeout=30,
                )
                (output / "palette-cli.log").write_text(
                    diagnostic.stdout + diagnostic.stderr
                )
                raise
            wait_for(
                "wallpaper presentation",
                lambda: matches(
                    "presentationState",
                    lambda row: (
                        row.get("activeReady") and row.get("pendingMatchesActive")
                    ),
                ),
                watch,
            )
            ipc("settingsApp", "close")
            wait_for(
                "settings close",
                lambda: matches("settingsApp", lambda row: not row.get("visible")),
                watch,
            )
            ipc("nacre", "close")
            # The normal unpinned dashboard intentionally closes after 140 ms
            # outside hover. It is exercised above; screenshots need the same
            # persistent preview API as the gallery, independent of CI timing.
            ipc("nacre", "previewDashboard", "0")
            wait_for(
                "dashboard screenshot state",
                lambda: matches(
                    "nacre", lambda row: row.get("dashboard") and row.get("tab") == 0
                ),
                watch,
            )
            screenshot = output / "screenshot.png"
            if args.compositor == "niri":
                subprocess.run(
                    [
                        "niri",
                        "msg",
                        "action",
                        "screenshot-screen",
                        "--path",
                        str(screenshot),
                    ],
                    env=env,
                    check=True,
                    capture_output=True,
                    timeout=20,
                )
            else:
                subprocess.run(
                    ["grim", str(screenshot)],
                    env=env,
                    check=True,
                    capture_output=True,
                    timeout=20,
                )
            wait_for(
                "screenshot",
                lambda: screenshot.is_file() and screenshot.stat().st_size > 1000,
                watch,
            )
            text = (output / "shell.log").read_text()
            diagnostics = [
                line
                for line in text.splitlines()
                if re.search(
                    r"(?:WARN|ERROR|FATAL).*qml|(?:file|qrc):.*\.qml:\d|QQml.*(?:error|warning)|Failed to load configuration",
                    line,
                    re.IGNORECASE,
                )
            ]
            if diagnostics:
                raise RuntimeError(
                    "QML diagnostics in prepared shell:\n" + "\n".join(diagnostics[:8])
                )
            (output / "result.json").write_text(
                json.dumps(
                    {
                        "compositor": args.compositor,
                        "seconds": round(time.monotonic() - started, 2),
                        "ipc": ["launcher", "dashboard", "settings", "wallpaper"],
                        "qmlDiagnostics": 0,
                    },
                    indent=2,
                )
                + "\n"
            )
            print("Fresh-install IPC and screenshot passed", flush=True)
        finally:
            if "snapshots" in locals():
                (output / "ipc-states.json").write_text(
                    json.dumps(snapshots, indent=2) + "\n"
                )
            for process in reversed(children):
                stop(process)
            for log in logs:
                log.close()
            (output / "dbus.log").unlink(missing_ok=True)


if __name__ == "__main__":
    main()
