#!/usr/bin/env python3
"""Opt-in real Niri 2/3-output proof with private HOME, bus, outputs and clients.

--driver is built from monitor_workspaces_headless.rs against the tested Niri
library. This never selects or alters an existing desktop's IPC socket.
"""

import argparse
import contextlib
import json
import os
from pathlib import Path
import select
import signal
import socket
import subprocess
import tempfile
import time

REPO = Path(__file__).resolve().parents[2]


def probe(driver, helper, count, output):
    processes = []
    report = {}
    with (
        tempfile.TemporaryDirectory(prefix="wm-") as folder,
        output.with_suffix(f".{count}.log").open("w") as log,
    ):
        home = Path(folder)
        runtime = home / "r"
        runtime.mkdir(mode=0o700)
        config = home / ".config/nacre/niri"
        config.mkdir(parents=True)
        names = {
            f"headless-{i}": [f"monitor-{i}-{n}" for n in range(1, 7)]
            for i in range(1, count + 1)
        }
        workspace_file = config / "workspaces.kdl"
        workspace_file.write_text(
            "".join(
                f"workspace {json.dumps(name)} {{ open-on-output {json.dumps(monitor)}; }}\n"
                for monitor, values in names.items()
                for name in values
            )
        )
        (config.parent / "profile.json").write_text(
            json.dumps({"version": 1, "workspaceCount": 6, "components": ["shell"]})
        )
        desktop = config.parent / "desktop.json"
        desktop.write_text(
            json.dumps(
                {
                    "monitorWorkspaceActions": True,
                    "niriWorkspaceOutputs": names,
                    "unrelatedPreference": "preserved",
                }
            )
        )
        loader = config / "config.kdl"
        loader.write_text(
            f'include "workspaces.kdl"\ninclude {json.dumps(str(REPO / "niri/bindings.kdl"))}\ninclude {json.dumps(str(REPO / "niri/monitor-workspace-actions.kdl"))}\n'
            + "\n".join(
                f'output "headless-{i}" {{ position x={(i - 1) * 800} y=0; }}'
                for i in range(1, count + 1)
            )
            + '\nwindow-rule { match title="Monitor Alpha"; open-on-workspace "monitor-1-1"; }\nwindow-rule { match title="Monitor Beta"; open-on-workspace "monitor-2-2"; }\nhotkey-overlay { skip-at-startup; }\n'
        )
        env = dict(
            os.environ,
            HOME=str(home),
            XDG_RUNTIME_DIR=str(runtime),
            XDG_CONFIG_HOME=str(home / ".config"),
            XDG_DATA_HOME=str(home / ".local/share"),
            XDG_STATE_HOME=str(home / ".local/state"),
            XDG_CACHE_HOME=str(home / ".cache"),
            NIRI_CONFIG=str(loader),
            NACRE_COMPOSITOR="niri",
            NACRE_HELPER_BINARY=str(helper),
            NACRE_COMPOSITOR_TOOLS=str(home / "tools"),
            DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + str(home / "absent-system-bus"),
            LIBGL_ALWAYS_SOFTWARE="1",
            EGL_PLATFORM="surfaceless",
            NIRI_DISABLE_SYSTEM_MANAGER_NOTIFY="1",
        )
        for key in (
            "NIRI_SOCKET",
            "WAYLAND_DISPLAY",
            "WAYLAND_SOCKET",
            "DISPLAY",
            "HYPRLAND_INSTANCE_SIGNATURE",
            "DBUS_SESSION_BUS_ADDRESS",
            "NACRE_QS_CONFIG",
            "NACRE_NIRI_CONFIG",
            "NACRE_NIRI_DESKTOP_CONFIG",
        ):
            env.pop(key, None)
        (home / "tools").mkdir()
        (home / "tools/nacre-helper").symlink_to(helper)

        def start(command, *, pipes=False, driver_environment=False):
            selected = dict(env)
            if not driver_environment:
                selected.pop("LD_LIBRARY_PATH", None)
            child = subprocess.Popen(
                command,
                env=selected,
                stdout=subprocess.PIPE if pipes else log,
                stderr=log,
                text=True,
                start_new_session=True,
            )
            processes.append(child)
            return child

        def line(child):
            assert select.select([child.stdout], [], [], 15)[0], (
                "Private process did not become ready"
            )
            value = child.stdout.readline().strip()
            assert value, "Private process exited before becoming ready"
            return value

        def run(command, check=True):
            result = subprocess.run(
                command, env=env, capture_output=True, text=True, timeout=15
            )
            if check and result.returncode:
                raise AssertionError(result.stderr[-6000:] + result.stdout[-2000:])
            return result

        def request(value):
            assert Path(env["NIRI_SOCKET"]).parent == runtime, (
                "Rejected non-private IPC socket"
            )
            with socket.socket(socket.AF_UNIX) as client:
                client.settimeout(5)
                client.connect(env["NIRI_SOCKET"])
                client.sendall((json.dumps(value) + "\n").encode())
                with client.makefile("r") as stream:
                    response = json.loads(stream.readline())
            assert "Err" not in response, response
            return response["Ok"]

        def action(name, **params):
            return request({"Action": {name: params}})

        def rows():
            return request("Workspaces")["Workspaces"]

        def windows():
            return request("Windows")["Windows"]

        def sets():
            values = rows()
            return {
                name: [
                    r["name"]
                    for r in sorted(values, key=lambda r: r["idx"])
                    if r["output"] == name and r["name"]
                ]
                for name in names
            }

        def controls(*args, check=True):
            return run([str(helper), "monitor-workspaces", *args], check=check)

        def wait_for(callback):
            deadline = time.monotonic() + 12
            while True:
                value = callback()
                if value:
                    return value
                assert time.monotonic() < deadline, "Native state did not settle"
                time.sleep(0.05)

        def open_overview():
            if not request("OverviewState")["OverviewState"]["is_open"]:
                action("ToggleOverview")

        def stop(child):
            if child.poll() is None:
                os.killpg(child.pid, signal.SIGTERM)
                try:
                    child.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    os.killpg(child.pid, signal.SIGKILL)
                    child.wait(timeout=3)

        try:
            bus_config = home / "bus.conf"
            bus_config.write_text(
                '<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>'
            )
            bus = start(
                [
                    "dbus-daemon",
                    "--nofork",
                    "--print-address=1",
                    "--config-file=" + str(bus_config),
                ],
                pipes=True,
            )
            env["DBUS_SESSION_BUS_ADDRESS"] = line(bus)
            run(["niri", "validate", "--config", str(loader)])

            def compositor():
                child = start(
                    [str(driver), str(loader), str(count)],
                    pipes=True,
                    driver_environment=True,
                )
                env["WAYLAND_DISPLAY"], env["NIRI_SOCKET"] = line(child).split("\t", 1)
                assert Path(env["NIRI_SOCKET"]).parent == runtime
                wait_for(lambda: len(request("Outputs")["Outputs"]) == count)
                return child

            current = compositor()
            # Niri removes NIRI_CONFIG from children; the managed session keeps this alias.
            env["NACRE_NIRI_CONFIG"] = str(loader)
            env.pop("NIRI_CONFIG", None)
            assert sets() == names, (sets(), names)
            fixture = home / "client.qml"
            fixture.write_text(
                'import Quickshell\nimport QtQuick\nShellRoot { FloatingWindow { visible: true; title: "Monitor Alpha"; implicitWidth: 320; implicitHeight: 200; Rectangle { anchors.fill: parent; color: "#557799" } } FloatingWindow { visible: true; title: "Monitor Beta"; implicitWidth: 320; implicitHeight: 200; Rectangle { anchors.fill: parent; color: "#997755" } } }\n'
            )
            env.update(QT_QPA_PLATFORM="wayland", QT_QUICK_BACKEND="software")
            clients = start(["quickshell", "-p", str(fixture)])
            values = wait_for(lambda: windows() if len(windows()) == 2 else None)
            alpha = next(w for w in values if w["title"] == "Monitor Alpha")
            action("FocusWindow", id=alpha["id"])
            open_overview()
            before_focus = next(r["id"] for r in rows() if r["is_focused"])
            target = next(
                r["id"] for r in rows() if r["output"] == "headless-2" and r["idx"] == 3
            )
            controls("send", "next", "3")
            wait_for(
                lambda: (
                    next(w["workspace_id"] for w in windows() if w["id"] == alpha["id"])
                    == target
                )
            )
            assert next(r["id"] for r in rows() if r["is_focused"]) == before_focus
            assert request("OverviewState")["OverviewState"]["is_open"]
            action("FocusWindow", id=alpha["id"])
            open_overview()
            controls("send", "previous", "1")
            wait_for(
                lambda: (
                    next(w["workspace_id"] for w in windows() if w["id"] == alpha["id"])
                    == alpha["workspace_id"]
                )
            )
            report["tileSendInOverview"] = True

            for i in reversed(range(1, count + 1)):
                action(
                    "FocusWorkspace", reference={"Name": names[f"headless-{i}"][i - 1]}
                )
            open_overview()
            original = sets()
            original_windows = {w["id"]: w["workspace_id"] for w in windows()}
            controls("swap", "next")
            exchanged = sets()
            assert exchanged["headless-1"][0] == original["headless-2"][1]
            assert exchanged["headless-2"][1] == original["headless-1"][0]
            assert all(len(values) == 6 for values in exchanged.values())
            assert original_windows == {w["id"]: w["workspace_id"] for w in windows()}
            controls("swap", "next")
            assert sets() == original
            assert request("OverviewState")["OverviewState"]["is_open"]
            report["selectedWorkspaceExchange"] = True
            controls("rotate", "next")
            rotated = sets()
            assert all(
                rotated[f"headless-{i % count + 1}"] == original[f"headless-{i}"]
                for i in range(1, count + 1)
            )
            assert original_windows == {w["id"]: w["workspace_id"] for w in windows()}
            controls("rotate", "previous")
            assert sets() == original
            assert json.loads(desktop.read_text())["unrelatedPreference"] == "preserved"
            report["allSetsForwardAndBackward"] = True

            valid_loader = loader.read_text()
            previous_fragment = workspace_file.read_bytes()
            previous_preferences = desktop.read_bytes()
            loader.write_text(valid_loader + "\ninvalid-owned-probe-node\n")
            try:
                failed = controls("swap", "next", check=False)
                assert failed.returncode != 0
                assert "restored" in failed.stderr, failed.stderr
                assert sets() == original
                assert workspace_file.read_bytes() == previous_fragment
                assert desktop.read_bytes() == previous_preferences
            finally:
                loader.write_text(valid_loader)
            report["validationFailureRollsBack"] = True

            controls("rotate", "next")
            saved = json.loads(desktop.read_text())["niriWorkspaceOutputs"]
            stop(clients)
            stop(current)
            compositor()
            assert sets() == saved
            report["restartRetainsAssignments"] = True
            return report
        finally:
            for child in reversed(processes):
                with contextlib.suppress(ProcessLookupError):
                    stop(child)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--driver", type=Path, required=True)
    parser.add_argument("--helper", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    result = {
        str(count): probe(
            args.driver.resolve(), args.helper.resolve(), count, args.output
        )
        for count in (2, 3)
    }
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
