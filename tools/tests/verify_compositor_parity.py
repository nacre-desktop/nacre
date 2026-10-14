#!/usr/bin/env python3
"""Opt-in nested parity probe. Never selects a running compositor's IPC socket."""

import argparse
import contextlib
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time

REPO = Path(__file__).resolve().parents[2]


def probe(repo, backend, output):
    results = {}
    processes = []
    parent_runtime = os.environ["XDG_RUNTIME_DIR"]
    parent_display = os.environ["WAYLAND_DISPLAY"]
    if not parent_display.startswith("/"):
        parent_display = str(Path(parent_runtime) / parent_display)
    with (
        tempfile.TemporaryDirectory(prefix="np-") as folder,
        output.with_suffix(".log").open("w") as log,
    ):
        home = Path(folder)
        runtime = home / "r"
        runtime.mkdir(mode=0o700)
        env = dict(
            os.environ,
            HOME=str(home),
            XDG_RUNTIME_DIR=str(runtime),
            XDG_CONFIG_HOME=str(home / ".config"),
            XDG_DATA_HOME=str(home / ".local/share"),
            XDG_STATE_HOME=str(home / ".local/state"),
            XDG_CACHE_HOME=str(home / ".cache"),
            WAYLAND_DISPLAY=parent_display,
            QT_QPA_PLATFORM="wayland",
            NACRE_COMPOSITOR=backend,
            NACRE_COMPOSITOR_TOOLS=str(repo / "nacre/shell-tools"),
            NACRE_PEARL_TOOLS=str(repo / "nacre/shell-tools"),
            DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + str(home / "no-system-bus"),
        )
        for key in (
            "NIRI_SOCKET",
            "HYPRLAND_INSTANCE_SIGNATURE",
            "DISPLAY",
            "NIRI_CONFIG",
            "NACRE_QS_CONFIG",
            "NACRE_NIRI_CONFIG",
            "NACRE_NIRI_DESKTOP_CONFIG",
        ):
            env.pop(key, None)
        bus_config = home / "bus.conf"
        bus_config.write_text(
            '<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>'
        )

        def start(args):
            process = subprocess.Popen(
                args, env=env, stdout=log, stderr=log, start_new_session=True
            )
            processes.append(process)
            return process

        def run(args, timeout=10):
            return subprocess.run(
                args,
                env=env,
                capture_output=True,
                text=True,
                timeout=timeout,
                check=True,
            )

        def facade(*args):
            return run([str(repo / "target/release/nacre-helper"), "compositor", *args])

        def query(name):
            return json.loads(facade("query", name).stdout)

        def check(name, callback):
            try:
                value = callback()
                results[name] = {"status": "passed", "evidence": value}
            except (
                OSError,
                ValueError,
                RuntimeError,
                AssertionError,
                StopIteration,
                subprocess.SubprocessError,
            ) as error:
                # Raw stderr may contain private paths; retain it only in local logs.
                print(name, repr(error), file=log)
                results[name] = {"status": "failed", "error": type(error).__name__}

        try:
            bus = subprocess.Popen(
                [
                    "dbus-daemon",
                    "--nofork",
                    "--print-address=1",
                    "--config-file=" + str(bus_config),
                ],
                stdout=subprocess.PIPE,
                stderr=log,
                text=True,
                start_new_session=True,
            )
            processes.append(bus)
            env["DBUS_SESSION_BUS_ADDRESS"] = bus.stdout.readline().strip()
            if not env["DBUS_SESSION_BUS_ADDRESS"]:
                raise RuntimeError("Private bus did not start")
            config = home / "config.kdl"
            desktop = home / "desktop.kdl"
            desktop.write_text("")
            if backend == "niri":
                config.write_text(
                    'include "'
                    + str(repo / "niri/bindings.kdl")
                    + '"\ninclude "'
                    + str(repo / "niri/window-rules.kdl")
                    + '"\ninclude "desktop.kdl"\nhotkey-overlay { skip-at-startup; }\n'
                )
                env.update(
                    NIRI_CONFIG=str(config),
                    NACRE_NIRI_CONFIG=str(config),
                    NACRE_NIRI_DESKTOP_CONFIG=str(desktop),
                )
                compositor = start(["/usr/bin/niri", "-c", str(config)])
                pattern = "niri.*.sock"
            else:
                config.write_text(
                    "monitor = ,1280x800,auto,1\nmisc {\n disable_hyprland_logo = true\n disable_splash_rendering = true\n}\n"
                )
                env["AQ_BACKEND"] = "wayland"
                compositor = start(["Hyprland", "-c", str(config)])
                pattern = "hypr/*/.socket.sock"
            deadline = time.monotonic() + 15
            while True:
                sockets = list(runtime.glob(pattern))
                displays = [
                    path for path in runtime.glob("wayland-*") if path.is_socket()
                ]
                if sockets and displays:
                    break
                if compositor.poll() is not None or time.monotonic() >= deadline:
                    raise RuntimeError("Nested compositor did not start")
                time.sleep(0.1)
            if backend == "niri":
                env["NIRI_SOCKET"] = str(sockets[0])
            else:
                env["HYPRLAND_INSTANCE_SIGNATURE"] = sockets[0].parent.name
            env["WAYLAND_DISPLAY"] = str(displays[0])
            for title in ("Parity One", "Parity Two"):
                start(
                    [
                        "kitty",
                        "--config",
                        "NONE",
                        "--title",
                        title,
                        "sh",
                        "-c",
                        "sleep 180",
                    ]
                )
            time.sleep(2)
            windows = query("windows")
            assert len(windows) == 2
            identifier = windows[0]["windowId"]
            if backend == "niri":

                def tabs():
                    facade("action", "focus_window", "--window", identifier)
                    run(
                        ["/usr/bin/niri", "msg", "action", "consume-window-into-column"]
                    )
                    run(
                        [
                            "/usr/bin/niri",
                            "msg",
                            "action",
                            "toggle-column-tabbed-display",
                        ]
                    )
                    run(["/usr/bin/niri", "msg", "action", "focus-window-down-or-top"])
                    assert query("active_window")["windowId"] != identifier
                    facade("action", "focus_window", "--window", identifier)
                    return "two windows in a tabbed column; tab focus changed"

                check("tabbed-column", tabs)
            results["window-state"] = {"status": "passed", "count": len(windows)}
            for name, args in (
                ("focus_window", ["--window", identifier]),
                ("float_window", ["--window", identifier, "--action", "enable"]),
                (
                    "resize_window",
                    ["--window", identifier, "--width", "500", "--height", "300"],
                ),
                ("move_window", ["--window", identifier, "--x", "50", "--y", "60"]),
                ("switch_workspace", ["--workspace", "2"]),
                ("move_window-workspace", ["--window", identifier, "--workspace", "2"]),
                ("fullscreen_window", ["--window", identifier, "--mode", "maximized"]),
            ):
                check(
                    name,
                    lambda name=name, args=args: (
                        facade("action", name.split("-workspace")[0], *args).returncode
                    ),
                )
                time.sleep(0.2)
            results["workspace-state"] = {
                "status": "passed",
                "count": len(query("workspaces")),
            }

            def screenshot():
                run(["grim", str(home / "capture.png")], timeout=5)
                assert (home / "capture.png").read_bytes().startswith(b"\x89PNG")
                return "PNG"

            check("grim", screenshot)
            protocols = run(["wayland-info"]).stdout
            results["protocols"] = {
                name: name in protocols
                for name in (
                    "ext_background_effect_manager_v1",
                    "zwlr_gamma_control_manager_v1",
                    "ext_session_lock_manager_v1",
                    "ext_idle_notifier_v1",
                    "zwlr_screencopy_manager_v1",
                )
            }
            if backend == "niri":
                for action in (
                    "toggle-column-tabbed-display",
                    "focus-window-up-or-bottom",
                    "focus-window-down-or-top",
                    "toggle-overview",
                    "toggle-overview",
                ):
                    check(
                        action,
                        lambda action=action: (
                            run(["/usr/bin/niri", "msg", "action", action]).returncode
                        ),
                    )

                def window_capture():
                    run(
                        [
                            "/usr/bin/niri",
                            "msg",
                            "action",
                            "screenshot-window",
                            "--id",
                            identifier.split(":")[1],
                            "--path",
                            str(home / "window.png"),
                        ]
                    )
                    deadline = time.monotonic() + 5
                    while (
                        not (home / "window.png").exists()
                        and time.monotonic() < deadline
                    ):
                        time.sleep(0.05)
                    assert (home / "window.png").read_bytes().startswith(b"\x89PNG")
                    return "PNG"

                check("native-window-capture", window_capture)
            if backend == "niri":

                def output_scale():
                    original = query("monitors")
                    plan = [dict(row) for row in original]
                    plan[0]["scale"] = 1.25
                    try:
                        facade(
                            "action",
                            "configure_monitors",
                            "--records",
                            json.dumps(plan),
                        )
                        time.sleep(0.25)
                        assert query("monitors")[0]["scale"] == 1.25
                    finally:
                        facade(
                            "action",
                            "configure_monitors",
                            "--records",
                            json.dumps(original),
                        )
                    return "1 -> 1.25 -> original"

                check("output-scale-roundtrip", output_scale)

            def x11():
                command = [
                    "kitty",
                    "--config",
                    "NONE",
                    "-o",
                    "linux_display_server=x11",
                    "--title",
                    "Parity X11",
                    "sh",
                    "-c",
                    "sleep 120",
                ]
                if backend == "niri":
                    run(["/usr/bin/niri", "msg", "action", "spawn", "--", *command])
                else:
                    import shlex

                    run(
                        [
                            "hyprctl",
                            "-i",
                            env["HYPRLAND_INSTANCE_SIGNATURE"],
                            "dispatch",
                            "exec",
                            shlex.join(command),
                        ]
                    )
                deadline = time.monotonic() + 5
                while True:
                    window = next(
                        (
                            row
                            for row in query("windows")
                            if row["title"] == "Parity X11"
                        ),
                        None,
                    )
                    if window:
                        break
                    if time.monotonic() >= deadline:
                        raise AssertionError("Forced-X11 test window did not open")
                    time.sleep(0.1)
                facade("action", "focus_window", "--window", window["windowId"])
                facade("action", "close_window", "--window", window["windowId"])
                return "forced-X11 window opened, focused and closed"

            check("x11", x11)
            tools = home / ".local/share/nacre/shell/tools"
            tools.parent.mkdir(parents=True)
            tools.symlink_to(repo / "nacre/shell-tools")
            interpreter = home / ".local/share/nacre/palette-runtime/venv/bin/python"
            interpreter.parent.mkdir(parents=True)
            interpreter.symlink_to(repo / ".venv/bin/python")
            preferences = home / ".config/nacre"
            preferences.mkdir(parents=True, exist_ok=True)
            (preferences / "profile.json").write_text('{"version":1}')
            shell = start(
                ["quickshell", "-p", str(repo / "nacre/shell/shell.qml"), "-n"]
            )
            time.sleep(5)
            assert shell.poll() is None

            def ipc(target, method, *args):
                return run(
                    [
                        "quickshell",
                        "-p",
                        str(repo / "nacre/shell/shell.qml"),
                        "ipc",
                        "call",
                        target,
                        method,
                        *args,
                    ]
                ).stdout.strip()

            def panel(method, args, field, expected=True):
                ipc("nacre", method, *args)
                time.sleep(0.4)
                state = json.loads(ipc("nacre", "state"))
                assert state[field] == expected
                return {field: state[field]}

            check("launcher", lambda: panel("mode", ["apps"], "launcher"))
            check("clipboard", lambda: panel("mode", ["clipboard"], "launcher"))
            check("palette", lambda: panel("mode", ["palette"], "launcher"))
            check("wallpaper-picker", lambda: panel("mode", ["wallpaper"], "launcher"))
            check(
                "searchable-overview", lambda: panel("mode", ["overview"], "launcher")
            )
            check("controls", lambda: panel("controls", ["home"], "osd"))

            def osd():
                ipc("nacre", "close")
                ipc("nacre", "previewFeedback", "volume")
                time.sleep(0.15)
                shown = json.loads(ipc("nacre", "state"))["feedback"]
                assert shown
                return "visible"

            check("osd", osd)
            check("frame-hide", lambda: panel("hide", [], "hidden"))
            check("frame-show", lambda: panel("hide", [], "hidden", False))
            check("shell-close", lambda: panel("close", [], "launcher", False))
            check("drawer-IPC", lambda: ipc("drawers", "list").splitlines())
            check("pearls-IPC", lambda: json.loads(ipc("pearls", "state")))

            def pearl():
                ipc("nacre", "close")
                ipc("pearls", "open", "launcher")
                deadline = time.monotonic() + 8
                while True:
                    entries = json.loads(ipc("pearls", "state"))
                    native = next(
                        (
                            row
                            for row in query("windows")
                            if entries and row["title"] == entries[0]["title"]
                        ),
                        None,
                    )
                    if (
                        len(entries) == 1
                        and entries[0]["visible"]
                        and native
                        and native["floating"]
                    ):
                        break
                    if time.monotonic() >= deadline:
                        raise AssertionError(
                            "Pearl did not become a floating window: "
                            + ipc("pearls", "gesture")
                        )
                    time.sleep(0.2)
                assert native["floating"], "Pearl remained tiled"
                assert query("active_window")["windowId"] == native["windowId"], (
                    "Pearl did not receive native focus"
                )
                ipc("pearls", "dock", entries[0]["key"])
                time.sleep(0.5)
                assert json.loads(ipc("pearls", "state")) == [], "Pearl did not dock"
                return "floating, placed and docked"

            check("pearl-lifecycle", pearl)
            if backend == "niri":

                def preference(key, value):
                    result = json.loads(
                        run(
                            [
                                "python3",
                                str(repo / "nacre/shell-tools/desktop-settings.py"),
                                "set",
                                key,
                                json.dumps(value),
                            ]
                        ).stdout
                    )
                    assert not result.get("error")
                    assert result["data"][key] == value
                    return {key: value}

                check("desktop-preference-save", lambda: preference("dnd", True))
                check("desktop-effect-save", lambda: preference("rounding", 12))
                check(
                    "cycle-window",
                    lambda: (
                        facade(
                            "action", "cycle_window", "--direction", "next"
                        ).returncode
                    ),
                )
            if backend == "hyprland":

                def trash_undo():
                    facade("action", "focus_window", "--window", identifier)
                    worker = start(
                        ["bash", str(repo / "hypr/scripts/window-trash.sh"), "trash"]
                    )
                    assert worker.wait(timeout=5) == 0
                    hidden = next(
                        row for row in query("windows") if row["windowId"] == identifier
                    )
                    assert hidden["workspace"]["name"] == "special:trash"
                    run(
                        [
                            "bash",
                            str(repo / "hypr/scripts/window-trash.sh"),
                            "restore-last",
                        ]
                    )
                    restored = next(
                        row for row in query("windows") if row["windowId"] == identifier
                    )
                    assert restored["workspace"]["name"] != "special:trash"
                    with contextlib.suppress(ProcessLookupError):
                        os.killpg(worker.pid, signal.SIGTERM)
                    return "hidden and restored"

                check("trash-undo", trash_undo)

                def minimize():
                    facade("action", "focus_window", "--window", identifier)
                    run(
                        [
                            "python3",
                            str(repo / "hypr/scripts/window-hide.py"),
                            "hide",
                            "--window",
                            identifier,
                            "--quiet",
                        ]
                    )
                    hidden = next(
                        row for row in query("windows") if row["windowId"] == identifier
                    )
                    assert hidden["workspace"]["name"] == "special:hidden"
                    run(
                        [
                            "python3",
                            str(repo / "hypr/scripts/window-hide.py"),
                            "restore-last",
                            "--quiet",
                        ]
                    )
                    restored = next(
                        row for row in query("windows") if row["windowId"] == identifier
                    )
                    assert restored["workspace"]["name"] != "special:hidden"
                    return "hidden and restored"

                check("minimize", minimize)
            check("shell-PNG", screenshot)

            def idle():
                marker = home / "idle-fired"
                config = home / "idle.conf"
                config.write_text(
                    "listener {\n timeout = 1\n on-timeout = touch "
                    + str(marker)
                    + "\n}\n"
                )
                worker = start(["hypridle", "-c", str(config)])
                deadline = time.monotonic() + 5
                while (
                    not marker.exists()
                    and time.monotonic() < deadline
                    and worker.poll() is None
                ):
                    time.sleep(0.1)
                assert marker.exists()
                worker.terminate()
                return "private timeout callback"

            check("idle-notify", idle)

            def lock():
                config = home / "lock.conf"
                config.write_text(
                    "background {\n monitor =\n color = rgb(202020)\n}\ninput-field {\n monitor =\n size = 200, 50\n placeholder_text = Nested lock probe\n}\n"
                )
                worker = start(
                    [
                        "hyprlock",
                        "-c",
                        str(config),
                        "--grace",
                        "0",
                        "--immediate-render",
                        "-v",
                    ]
                )
                time.sleep(2)
                assert worker.poll() is None
                log.flush()
                assert "onLockLocked called" in output.with_suffix(".log").read_text()
                # Authentication and unlock deliberately remain owner-run checks.
                return (
                    "lock client remains connected; inspect local lock acknowledgement"
                )

            check("lock-surface", lock)

        finally:
            for process in reversed(processes):
                if process.poll() is None:
                    with contextlib.suppress(ProcessLookupError):
                        os.killpg(process.pid, signal.SIGTERM)
                    try:
                        process.wait(timeout=3)
                    except subprocess.TimeoutExpired:
                        os.killpg(process.pid, signal.SIGKILL)
                        process.wait()
    output.write_text(
        json.dumps(
            {
                "backend": backend,
                "dirty": subprocess.run(["git", "diff", "--quiet"], cwd=repo).returncode
                != 0,
                "revision": subprocess.check_output(
                    ["git", "rev-parse", "HEAD"], cwd=repo, text=True
                ).strip(),
                "results": results,
            },
            indent=2,
        )
        + "\n"
    )
    print(
        backend
        + ": "
        + ", ".join(
            name + "=" + value["status"]
            for name, value in results.items()
            if "status" in value
        )
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backend", choices=("hyprland", "niri"), required=True)
    parser.add_argument("--repo", type=Path, default=REPO)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    probe(args.repo.resolve(), args.backend, args.output)


if __name__ == "__main__":
    main()
