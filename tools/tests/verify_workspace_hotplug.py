#!/usr/bin/env python3
"""Owned headless niri topology transitions; never connects to the desktop socket."""

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


def probe(driver, helper, mode, output):
    children = []
    with tempfile.TemporaryDirectory(prefix="wh-") as folder, output.open("w") as log:
        home = Path(folder)
        runtime = home / "r"
        runtime.mkdir(mode=0o700)
        config = home / ".config/nacre/niri"
        config.mkdir(parents=True)
        names = {f"headless-{i}": [f"m{i}-{n}" for n in range(1, 7)] for i in (1, 2, 3)}
        fragment = config / "workspaces.kdl"
        fragment.write_text(
            "".join(
                f'workspace "{n}" {{ open-on-output "headless-1"; }}\n'
                for n in names["headless-1"]
            )
        )
        (config / "monitor-workspace-actions.kdl").write_text("")
        loader = config / "config.kdl"
        loader.write_text(
            'include "workspaces.kdl"\ninclude "monitor-workspace-actions.kdl"\nanimations { off; }\nhotkey-overlay { skip-at-startup; }\n'
            + "".join(
                f'output "headless-{i}" {{ position x={(i - 1) * 800} y=0; }}\n'
                for i in (1, 2, 3)
            )
        )
        (config.parent / "profile.json").write_text(
            json.dumps(
                {
                    "version": 1,
                    "workspaceCount": 6,
                    "workspaceNames": names["headless-1"],
                    "components": ["shell"],
                }
            )
        )
        original = json.dumps(
            {
                "version": 2,
                "niriWorkspaceOutputs": names,
                "niriWorkspaceMode": mode,
                "workspacePrimaryOutput": "headless-1",
                "monitorWorkspaceActions": True,
                "unrelated": "keep",
            }
        ).encode()
        desktop = config.parent / "desktop.json"
        desktop.write_bytes(original)
        env = dict(
            os.environ,
            HOME=str(home),
            XDG_CONFIG_HOME=str(home / ".config"),
            XDG_STATE_HOME=str(home / ".local/state"),
            XDG_DATA_HOME=str(home / ".local/share"),
            XDG_CACHE_HOME=str(home / ".cache"),
            XDG_RUNTIME_DIR=str(runtime),
            NACRE_COMPOSITOR="niri",
            NACRE_NIRI_CONFIG=str(loader),
            LIBGL_ALWAYS_SOFTWARE="1",
            EGL_PLATFORM="surfaceless",
            NIRI_DISABLE_SYSTEM_MANAGER_NOTIFY="1",
            QT_QPA_PLATFORM="wayland",
            QT_QUICK_BACKEND="software",
            DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + str(home / "no-system-bus"),
        )
        for key in (
            "NIRI_SOCKET",
            "NIRI_CONFIG",
            "WAYLAND_DISPLAY",
            "WAYLAND_SOCKET",
            "DISPLAY",
            "DBUS_SESSION_BUS_ADDRESS",
            "HYPRLAND_INSTANCE_SIGNATURE",
            "NACRE_QS_CONFIG",
            "NACRE_NIRI_DESKTOP_CONFIG",
        ):
            env.pop(key, None)

        def start(cmd, pipes=False, stdin=False, driver_env=False):
            selected = dict(env)
            if not driver_env:
                selected.pop("LD_LIBRARY_PATH", None)
            p = subprocess.Popen(
                cmd,
                env=selected,
                stdout=subprocess.PIPE if pipes else log,
                stderr=log,
                stdin=subprocess.PIPE if stdin else subprocess.DEVNULL,
                text=True,
                start_new_session=True,
            )
            children.append(p)
            return p

        def line(p):
            assert select.select([p.stdout], [], [], 15)[0], (
                "owned child readiness timeout"
            )
            value = p.stdout.readline().strip()
            assert value, "owned child exited"
            return value

        def wait(test):
            end = time.monotonic() + 10
            while True:
                result = test()
                if result:
                    return result
                assert time.monotonic() < end, "native state timeout; see " + str(
                    output
                )
                time.sleep(0.04)

        def request(value):
            assert Path(env["NIRI_SOCKET"]).parent == runtime
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

        def topology(command, outputs):
            compositor.stdin.write(command + "\n")
            compositor.stdin.flush()
            assert line(compositor) == "done " + command
            wait(lambda: set(request("Outputs")["Outputs"]) == set(outputs))
            wait(lambda: policy_ready(outputs))
            ledger = home / ".local/state/nacre/workspace-session.json"
            wait(
                lambda: (
                    ledger.exists()
                    and set(json.loads(ledger.read_text()).get("connected", []))
                    == set(outputs)
                )
            )

        def policy_ready(outputs):
            live = rows()
            for output_name in outputs:
                named = [
                    r
                    for r in live
                    if r["output"] == output_name and r["name"] in names[output_name]
                ]
                if mode == "fixed-all" or output_name == "headless-1":
                    if len(named) != 6:
                        return False
                elif named:
                    return False
            disconnected = set(names) - set(outputs)
            populated = {w["workspace_id"] for w in windows()}
            if any(
                r["name"]
                and r["id"] not in populated
                and any(r["name"] in names[o] for o in disconnected)
                for r in live
            ):
                return False
            return True

        def workspace(output_name, index):
            return next(
                r for r in rows() if r["output"] == output_name and r["idx"] == index
            )

        def run_bring():
            result = subprocess.run(
                [str(helper), "monitor-workspaces", "bring"],
                env=env,
                text=True,
                capture_output=True,
                timeout=12,
            )
            assert result.returncode == 0, result.stderr
            return json.loads(result.stdout)

        try:
            busfile = home / "bus.conf"
            busfile.write_text(
                '<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>'
            )
            env["DBUS_SESSION_BUS_ADDRESS"] = line(
                start(
                    [
                        "dbus-daemon",
                        "--nofork",
                        "--print-address=1",
                        "--config-file=" + str(busfile),
                    ],
                    pipes=True,
                )
            )
            compositor = start(
                [str(driver), str(loader), "1"], pipes=True, stdin=True, driver_env=True
            )
            env["WAYLAND_DISPLAY"], env["NIRI_SOCKET"] = line(compositor).split("\t")
            env["NACRE_NIRI_WORKSPACE_POLICY"] = "0"
            read_only = start([str(helper), "compositor", "events"], pipes=True)
            assert json.loads(line(read_only))["type"] == "snapshot"
            os.killpg(read_only.pid, signal.SIGTERM)
            read_only.wait(timeout=3)
            assert desktop.read_bytes() == original, (
                "Ordinary event queries must be read-only"
            )
            assert not (home / ".local/state/nacre/workspace-session.json").exists()
            env["NACRE_NIRI_WORKSPACE_POLICY"] = "1"
            stream = start([str(helper), "compositor", "events"])
            wait(lambda: desktop.with_name("desktop.json.pre-v2-to-v3.bak").exists())
            assert (
                desktop.with_name("desktop.json.pre-v2-to-v3.bak").read_bytes()
                == original
            )
            assert json.loads(desktop.read_text())["unrelated"] == "keep"
            assert "niriWorkspaceOutputs" not in json.loads(desktop.read_text())
            topology("add 2", ["headless-1", "headless-2"])
            topology("remove 2", ["headless-1"])
            topology("add 2 3", names)
            fixture = home / "clients.qml"
            fixture.write_text(
                "import Quickshell\nimport QtQuick\nShellRoot {"
                + "".join(
                    f'FloatingWindow {{ visible:true; title:"client-{i}"; implicitWidth:200; implicitHeight:150; }}'
                    for i in range(6)
                )
                + "}\n"
            )
            start(["quickshell", "-p", str(fixture)])
            live = wait(lambda: windows() if len(windows()) == 6 else None)
            pairs = {}
            for i, output_name in enumerate(names):
                target = workspace(
                    output_name, 2 if mode == "fixed-all" or i == 0 else 1
                )
                pair = sorted(live, key=lambda w: w["title"])[2 * i : 2 * i + 2]
                for w in pair:
                    action(
                        "MoveWindowToWorkspace",
                        window_id=w["id"],
                        reference={"Id": target["id"]},
                        focus=False,
                    )
                action("FocusWindow", id=pair[-1]["id"])
                pairs[output_name] = (target["id"], pair[-1]["id"])
            action("FocusWindow", id=pairs["headless-1"][1])
            time.sleep(0.25)
            topology("remove 2 3", ["headless-1"])
            assert len(windows()) == 6
            # Only occupied named visitors survive in fixed mode; none in dynamic mode.
            assert sum(bool(r["name"]) for r in rows()) == (
                8 if mode == "fixed-all" else 6
            )
            # Reconnect in the reverse order, retaining per-monitor view and selected tile.
            topology("add 3", ["headless-1", "headless-3"])
            topology("add 2", names)
            for output_name, (ws, window) in pairs.items():
                wait(
                    lambda ws=ws, output_name=output_name, window=window: any(
                        r["id"] == ws
                        and r["output"] == output_name
                        and r["is_active"]
                        and r["active_window_id"] == window
                        for r in rows()
                    )
                )
            # Do not wait for policy settlement between remove/re-add.
            compositor.stdin.write("remove 2\n")
            compositor.stdin.flush()
            assert line(compositor) == "done remove 2"
            topology("add 2", names)
            topology("remove 2 3", ["headless-1"])
            target = workspace("headless-1", 4)
            action("FocusWorkspace", reference={"Id": target["id"]})
            merged = run_bring()
            assert merged["movedWindows"] == 4, merged
            destination = (
                workspace("headless-1", 2)["id"]
                if mode == "fixed-all"
                else target["id"]
            )
            for w in windows():
                if w["id"] not in [
                    x["id"] for x in sorted(live, key=lambda w: w["title"])[:2]
                ]:
                    assert w["workspace_id"] == destination, (
                        mode,
                        merged,
                        destination,
                        w,
                        rows(),
                    )
            merged_windows = [w for w in windows() if w["workspace_id"] == destination]
            columns = [
                w["layout"]["pos_in_scrolling_layout"][0] for w in merged_windows
            ]
            assert len(set(columns)) == len(merged_windows), (
                "Merged tiles must remain separate columns"
            )
            if mode == "fixed-all":
                local_columns = [
                    w["layout"]["pos_in_scrolling_layout"][0]
                    for w in merged_windows
                    if w["title"] in ("client-0", "client-1")
                ]
                incoming_columns = [
                    w["layout"]["pos_in_scrolling_layout"][0]
                    for w in merged_windows
                    if w["title"] not in ("client-0", "client-1")
                ]
                assert min(incoming_columns) > max(local_columns), (
                    "Append after existing columns"
                )
            topology("add 3 2", names)
            assert all(
                w["workspace_id"] == destination
                for w in windows()
                if w["title"] not in ("client-0", "client-1")
            )
            assert len(windows()) == 6
            if mode == "fixed-all":
                for output_name in ("headless-2", "headless-3"):
                    action("FocusWorkspace", reference={"Name": names[output_name][3]})
                action("FocusWorkspace", reference={"Id": target["id"]})
                time.sleep(0.2)
                topology("remove 2 3", ["headless-1"])
                topology("add 3 2", names)
                for output_name in ("headless-2", "headless-3"):
                    wait(
                        lambda output_name=output_name: any(
                            r["output"] == output_name
                            and r["name"] == names[output_name][3]
                            and r["is_active"]
                            for r in rows()
                        )
                    )
            else:
                # Adopt the selected visitor itself, including its workspace home.
                moving = next(w for w in windows() if w["title"] == "client-2")
                external = workspace("headless-2", 1)
                action(
                    "MoveWindowToWorkspace",
                    window_id=moving["id"],
                    reference={"Id": external["id"]},
                    focus=False,
                )
                action("FocusWindow", id=moving["id"])
                time.sleep(0.2)
                topology("remove 2", ["headless-1", "headless-3"])
                action("FocusWindow", id=moving["id"])
                run_bring()
                topology("add 2", names)
                wait(
                    lambda: any(
                        r["id"] == external["id"] and r["output"] == "headless-1"
                        for r in rows()
                    )
                )
            if mode == "primary-dynamic":
                ledger_path = home / ".local/state/nacre/workspace-session.json"
                wait(
                    lambda: (
                        json.loads(ledger_path.read_text())["origins"].get(
                            "niri-workspace:" + str(external["id"]), [""]
                        )[0]
                        == "headless-1"
                    )
                )
            if mode == "primary-dynamic":
                moving = next(w for w in windows() if w["title"] == "client-4")
                target_dynamic = workspace("headless-3", 1)
                action(
                    "MoveWindowToWorkspace",
                    window_id=moving["id"],
                    reference={"Id": target_dynamic["id"]},
                    focus=False,
                )
                wait(
                    lambda: len([r for r in rows() if r["output"] == "headless-3"]) == 2
                )
                empty = workspace("headless-3", 2)
                action("FocusWorkspace", reference={"Id": empty["id"]})
                action("FocusWorkspace", reference={"Id": target["id"]})
                time.sleep(0.2)
                topology("remove 3", ["headless-1", "headless-2"])
                topology("add 3", names)
                wait(
                    lambda: any(
                        r["output"] == "headless-3"
                        and r["idx"] == 2
                        and r["is_active"]
                        and r["active_window_id"] is None
                        for r in rows()
                    )
                )
            if mode == "primary-dynamic":
                # Settings writes the versioned preference, then its managed
                # desktop include produces ConfigLoaded. Exercise both directions.
                preferences = json.loads(desktop.read_text())
                preferences["niriWorkspaceMode"] = "fixed-all"
                desktop.write_text(json.dumps(preferences))
                action("LoadConfigFile", path=str(loader))
                wait(lambda: sum(bool(r["name"]) for r in rows()) == 18)
                wait(
                    lambda: (
                        "monitor-workspaces send"
                        in (config / "monitor-workspace-actions.kdl").read_text()
                    )
                )
                preferences["niriWorkspaceMode"] = "primary-dynamic"
                desktop.write_text(json.dumps(preferences))
                action("LoadConfigFile", path=str(loader))
                wait(lambda: sum(bool(r["name"]) for r in rows()) == 6)
                wait(lambda: not (config / "monitor-workspace-actions.kdl").read_text())
                disabled = subprocess.run(
                    [str(helper), "monitor-workspaces", "plan", "swap", "next"],
                    env=env,
                    capture_output=True,
                    text=True,
                    timeout=10,
                )
                assert disabled.returncode != 0 and "fixed sets" in disabled.stderr
            assert stream.poll() is None
            return {
                "migrationBackup": True,
                "readOnlyEvents": True,
                "1-2-1": True,
                "1-3-1": True,
                "simultaneousUndock": True,
                "reverseReconnect": True,
                "activeWorkspaceAndWindow": True,
                "quickBlip": True,
                "mergeStaysAfterReconnect": True,
                "appendedColumns": True,
                "emptySlotOrSelectedVisitor": True,
                **(
                    {"settingsModeRoundtrip": True} if mode == "primary-dynamic" else {}
                ),
            }
        except Exception:
            log.write(
                "\nPOLICY_CONFIG "
                + fragment.read_text()
                + "\nPOLICY_DESKTOP "
                + desktop.read_text()
                + "\n"
            )
            log.flush()
            raise
        finally:
            for p in reversed(children):
                if p.poll() is None:
                    with contextlib.suppress(ProcessLookupError):
                        os.killpg(p.pid, signal.SIGTERM)
                    try:
                        p.wait(timeout=3)
                    except subprocess.TimeoutExpired:
                        os.killpg(p.pid, signal.SIGKILL)
                        p.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--driver", type=Path, required=True)
    parser.add_argument("--helper", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    result = {
        mode: probe(
            args.driver.resolve(),
            args.helper.resolve(),
            mode,
            args.output.with_suffix("." + mode + ".log"),
        )
        for mode in ("primary-dynamic", "fixed-all")
    }
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
