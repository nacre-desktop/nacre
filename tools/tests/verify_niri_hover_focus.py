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

from PIL import Image

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
            'import Quickshell\nimport Quickshell.Io\nimport Quickshell.Wayland\nimport QtQuick\nShellRoot {\nPanelWindow { id: overlay; visible: false; anchors.top: true; anchors.right: true; implicitWidth: 120; implicitHeight: 80; exclusiveZone: 0; WlrLayershell.keyboardFocus: WlrKeyboardFocus.OnDemand; color: "#222222" }\nIpcHandler { target: "hoverFocus"; function panel(value: bool): void { overlay.visible = value; } }\n'
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
                        key=lambda entry: entry["layout"]["pos_in_scrolling_layout"],
                    )

                def positions():
                    value = json.loads((root / "geometry.json").read_text())
                    by_id = {row[0]: row[1:] for row in value["tiles"]}
                    return [by_id[w["id"]] for w in windows()]

                def focused():
                    return next((w["id"] for w in windows() if w["is_focused"]), None)

                def move(x, y=300, env=env):
                    selected = dict(env)
                    selected.pop("LD_LIBRARY_PATH", None)
                    subprocess.run(
                        [str(pointer), str(int(x)), str(int(y)), "800", "600"],
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
                    # Hovering the visible part of an overflowing tile must focus it,
                    # rather than reject it as stock max-scroll-amount=0 does.
                    move(before[2][0] + 25)
                    assert focused() == third and positions() == before, (
                        name,
                        "Partial tile hover failed",
                        focused(),
                        third,
                        before,
                        positions(),
                    )
                    # Pointer motion within a tile must reclaim selection after keyboard use.
                    action("FocusColumnFirst")
                    move(before[2][0] + 28)
                    assert focused() == third and positions() == before, (
                        name,
                        "Same-tile pointer motion failed",
                    )
                    move(before[0][0] + 80)
                    assert focused() == first and positions() == before
                    panel_env = dict(env)
                    panel_env.pop("LD_LIBRARY_PATH", None)

                    def panel(visible, panel_env=panel_env):
                        subprocess.run(
                            [
                                "quickshell",
                                "-p",
                                str(fixture),
                                "ipc",
                                "call",
                                "hoverFocus",
                                "panel",
                                str(visible).lower(),
                            ],
                            env=panel_env,
                            check=True,
                            capture_output=True,
                        )
                        time.sleep(0.15)

                    panel(True)
                    move(740, 30)
                    assert request("FocusedWindow")["FocusedWindow"] is None, (
                        "Panel did not take keyboard focus"
                    )
                    move(before[0][0] + 80)
                    assert focused() == first and positions() == before, (
                        "Hover did not reclaim focus from panel"
                    )
                    panel(False)
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
                    # Bird's Eye hover changes the real layout selection without moving
                    # either tiles or workspace rows. App keyboard focus stays cleared.
                    action("ToggleOverview")

                    def geometry():
                        return json.loads((root / "geometry.json").read_text())

                    view = geometry()["visible"]
                    targets = {row[0]: row for row in view}
                    target = targets[third]
                    selected = dict(env)
                    selected.pop("LD_LIBRARY_PATH", None)

                    def point(x, y, selected=selected):
                        subprocess.run(
                            [str(pointer), str(int(x)), str(int(y)), "800", "600"],
                            env=selected,
                            check=True,
                        )
                        time.sleep(0.15)

                    point(target[1] + 25, target[2] + target[4] / 2)
                    assert geometry()["selected"] == third, (
                        name,
                        "Overview hover did not select",
                    )
                    assert {r[0]: r[1:] for r in geometry()["visible"]} == {
                        r[0]: r[1:] for r in view
                    }, (
                        name,
                        "Overview hover moved view",
                    )
                    assert request("FocusedWindow")["FocusedWindow"] is None
                    # An implicit native workspace move must now target the hovered tile.
                    action(
                        "MoveWindowToWorkspace",
                        window_id=None,
                        reference={"Index": 2},
                        focus=False,
                    )
                    assert (
                        next(
                            row["workspace_id"]
                            for row in windows()
                            if row["id"] == third
                        )
                        != rows[2]["workspace_id"]
                    )
                    assert request("OverviewState")["OverviewState"]["is_open"]
                    cross_view = geometry()["visible"]
                    target = next(r for r in cross_view if r[0] == third)
                    assert target[2] < 598, (
                        "Destination workspace is not visible in overview"
                    )
                    point(target[1] + 25, min(target[2] + target[4] / 2, 580))
                    assert geometry()["selected"] == third, (
                        name,
                        "Other-workspace hover did not select",
                    )
                    assert {r[0]: r[1:] for r in geometry()["visible"]} == {
                        r[0]: r[1:] for r in cross_view
                    }, (name, "Workspace row moved on hover")
                    image_path = output.with_name(output.stem + "." + name + ".png")
                    action(
                        "ScreenshotScreen",
                        write_to_disk=True,
                        show_pointer=False,
                        path=str(image_path),
                    )
                    deadline = time.monotonic() + 5
                    while not image_path.exists():
                        assert time.monotonic() < deadline, (
                            "Native overview screenshot missing"
                        )
                        time.sleep(0.05)
                    pixels = Image.open(image_path).convert("RGB")
                    bright = []
                    for x in range(pixels.width):
                        for y in range(pixels.height):
                            red, green, blue = pixels.getpixel((x, y))
                            if green > 140 and blue > 180 and blue > red + 40:
                                bright.append((x, y))
                    assert bright, "Selected overview tile has no visible highlight"
                    assert all(
                        target[1] - 3 <= x <= target[1] + target[3] + 3
                        and target[2] - 3 <= y <= target[2] + target[4] + 3
                        for x, y in bright
                    ), "Another workspace still has a bright selection ring"
                    action("ToggleOverview")
                    assert focused() == third, "Closing overview lost hovered selection"
                    report[name] = {
                        "otherWorkspaceHoverSelects": True,
                        "otherWorkspaceViewStationary": True,
                        "singleBrightSelectionRing": True,
                        "exitRetainsSelection": True,
                        "hoverReclaimsPanelFocus": True,
                        "partialHoverFocus": True,
                        "sameTilePointerMotion": True,
                        "overviewHoverSelects": True,
                        "overviewViewStationary": True,
                        "overviewMoveTargetsHover": True,
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
