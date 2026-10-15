#!/usr/bin/env python3
"""Opt-in hover/edge regression against a private native Niri headless driver.

Use monitor_workspaces_headless.rs for --driver. The driver, virtual pointer and
clients use a disposable HOME, runtime directory and non-activating session bus.
No input or IPC is delivered to an existing desktop. The unrestricted control
must scroll; defaults must focus visible tiles without scrolling at either edge.
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

from verify_niri_overview_hover import build_pointer

REPO = Path(__file__).resolve().parents[2]


def verify(driver, helper, pointer, output):
    report = {}
    with tempfile.TemporaryDirectory(prefix="niri-focus-") as folder:
        root = Path(folder)
        runtime = root / "runtime"
        runtime.mkdir(mode=0o700)
        config = root / ".config/nacre/niri"
        config.mkdir(parents=True)
        defaults = json.loads(
            (REPO / "nacre/shell-tools/desktop-defaults.json").read_text()
        )
        assert defaults["followMouse"] is True
        # Render twice to cover a normal preference save that used to undo the fix.
        fragments = []
        for gaps in (10, 12):
            values = dict(defaults, gapsIn=gaps)
            text = subprocess.check_output(
                [
                    str(helper),
                    "compositor-render",
                    "niri",
                    "render_desktop_options",
                    json.dumps(values),
                ],
                text=True,
            )
            fragments.append(json.loads(text))
        bounded = 'focus-follows-mouse max-scroll-amount="0%"'
        assert all(bounded in value for value in fragments)
        pointer = pointer or build_pointer(root)
        loader = config / "config.kdl"
        fixture = root / "client.qml"
        fixture.write_text(
            "import Quickshell\nimport QtQuick\nShellRoot {\n"
            + "\n".join(
                f'FloatingWindow {{ visible: true; title: "Hover {name}"; implicitWidth: 350; implicitHeight: 200; Rectangle {{ anchors.fill: parent; color: "#557799" }} }}'
                for name in ("Alpha", "Beta", "Gamma")
            )
            + "\n}\n"
        )
        base_env = dict(
            os.environ,
            HOME=str(root),
            XDG_RUNTIME_DIR=str(runtime),
            XDG_CONFIG_HOME=str(root / ".config"),
            XDG_DATA_HOME=str(root / ".local/share"),
            XDG_STATE_HOME=str(root / ".local/state"),
            XDG_CACHE_HOME=str(root / ".cache"),
            DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + str(root / "absent-system-bus"),
            LIBGL_ALWAYS_SOFTWARE="1",
            EGL_PLATFORM="surfaceless",
            QT_QPA_PLATFORM="wayland",
            QT_QUICK_BACKEND="software",
        )
        for key in (
            "NIRI_SOCKET",
            "NIRI_CONFIG",
            "NACRE_NIRI_CONFIG",
            "NACRE_NIRI_DESKTOP_CONFIG",
            "WAYLAND_DISPLAY",
            "WAYLAND_SOCKET",
            "DISPLAY",
            "HYPRLAND_INSTANCE_SIGNATURE",
            "DBUS_SESSION_BUS_ADDRESS",
            "NACRE_QS_CONFIG",
        ):
            base_env.pop(key, None)
        bus_config = root / "bus.conf"
        bus_config.write_text(
            '<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>'
        )
        output.parent.mkdir(parents=True, exist_ok=True)
        with output.with_suffix(".log").open("w") as log:
            for name, fragment in (
                (
                    "unrestricted-control",
                    fragments[0].replace(bounded, "focus-follows-mouse"),
                ),
                ("general-default", fragments[0]),
                ("after-preference-regeneration", fragments[1]),
            ):
                processes = []
                env = dict(base_env)

                def start(
                    command,
                    *,
                    pipes=False,
                    driver_env=False,
                    env=env,
                    processes=processes,
                ):
                    child_env = dict(env)
                    if not driver_env:
                        child_env.pop("LD_LIBRARY_PATH", None)
                    child = subprocess.Popen(
                        command,
                        env=child_env,
                        stdout=subprocess.PIPE if pipes else log,
                        stderr=log,
                        text=True,
                        start_new_session=True,
                    )
                    processes.append(child)
                    return child

                def line(child):
                    assert select.select([child.stdout], [], [], 15)[0], (
                        "Private process not ready"
                    )
                    value = child.stdout.readline().strip()
                    assert value, "Private process exited"
                    return value

                def request(value, env=env):
                    assert Path(env["NIRI_SOCKET"]).parent == runtime
                    with socket.socket(socket.AF_UNIX) as client:
                        client.settimeout(5)
                        client.connect(env["NIRI_SOCKET"])
                        client.sendall((json.dumps(value) + "\n").encode())
                        with client.makefile("r") as stream:
                            response = json.loads(stream.readline())
                    assert "Ok" in response, response
                    return response["Ok"]

                def action(name, **params):
                    request({"Action": {name: params}})
                    time.sleep(0.15)

                def windows():
                    return sorted(
                        request("Windows")["Windows"],
                        key=lambda row: row["layout"]["pos_in_scrolling_layout"],
                    )

                def positions():
                    value = json.loads((root / "geometry.json").read_text())
                    by_id = {row[0]: row[1:] for row in value["tiles"]}
                    return [by_id[w["id"]] for w in windows()]

                def focused():
                    return next(w["id"] for w in windows() if w["is_focused"])

                def move(x, env=env):
                    selected = dict(env)
                    selected.pop("LD_LIBRARY_PATH", None)
                    subprocess.run(
                        [str(pointer), str(int(x)), "300", "800", "600"],
                        env=selected,
                        check=True,
                    )
                    time.sleep(0.15)

                try:
                    (config / "desktop.kdl").write_text(fragment)
                    loader.write_text(
                        'include "desktop.kdl"\n'
                        + '\nanimations { off; }\nlayout { default-column-width { fixed 350; }; center-focused-column "never"; }\nhotkey-overlay { skip-at-startup; }\n'
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
                    compositor = start(
                        [str(driver), str(loader), "1", str(root / "geometry.json")],
                        pipes=True,
                        driver_env=True,
                    )
                    env["WAYLAND_DISPLAY"], env["NIRI_SOCKET"] = line(compositor).split(
                        "\t", 1
                    )
                    start(["quickshell", "-p", str(fixture), "-n"])
                    deadline = time.monotonic() + 15
                    while len(windows()) != 3:
                        assert time.monotonic() < deadline, (
                            "Three private tiles did not map"
                        )
                        time.sleep(0.1)
                    action("FocusColumnFirst")
                    rows = windows()
                    first, second, third = [w["id"] for w in rows]
                    before = positions()
                    # Two whole tiles and a partial third must be in view.
                    x2 = before[1][0] + rows[1]["layout"]["tile_size"][0] / 2
                    assert 0 < before[0][0] < before[1][0] < before[2][0] < 799
                    assert before[2][0] + rows[2]["layout"]["tile_size"][0] > 800
                    move(before[0][0] + 80)
                    move(x2)
                    assert focused() == second, (
                        name,
                        "Visible hover did not focus",
                        windows(),
                    )
                    assert positions() == before, (name, "Visible hover scrolled")
                    move(799)
                    if name == "unrestricted-control":
                        assert focused() == third and positions() != before, (
                            "Control failed to reproduce edge scrolling"
                        )
                        report[name] = {"edgeScrollReproduced": True}
                        continue
                    assert focused() == second and positions() == before, (
                        name,
                        "Right edge scrolled",
                    )
                    move(before[0][0] + 80)
                    assert focused() == first and positions() == before
                    # These are the native actions used by directional keyboard bindings.
                    action("FocusColumnRight")
                    action("FocusColumnRight")
                    assert focused() == third and positions() != before, (
                        "Keyboard action failed to scroll"
                    )
                    right_view = positions()
                    # The partially hidden first tile must not pull the view back left.
                    move(1)
                    assert focused() == third and positions() == right_view, (
                        name,
                        "Left edge scrolled",
                    )
                    action("FocusColumnFirst")
                    assert focused() == first and positions() == before
                    report[name] = {
                        "visibleHoverFocus": True,
                        "rightEdgeStationary": True,
                        "leftEdgeStationary": True,
                        "keyboardNavigationScrolls": True,
                        "positions": before,
                    }
                finally:
                    for child in reversed(processes):
                        if child.poll() is None:
                            with contextlib.suppress(ProcessLookupError):
                                os.killpg(child.pid, signal.SIGTERM)
                            try:
                                child.wait(timeout=3)
                            except subprocess.TimeoutExpired:
                                os.killpg(child.pid, signal.SIGKILL)
                                child.wait(timeout=3)
    output.write_text(json.dumps(report, indent=2) + "\n")
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--driver", required=True, type=Path)
    parser.add_argument("--helper", required=True, type=Path)
    parser.add_argument("--pointer", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    print(
        json.dumps(
            verify(
                args.driver.resolve(),
                args.helper.resolve(),
                args.pointer,
                args.output.resolve(),
            ),
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
