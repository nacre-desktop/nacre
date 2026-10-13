#!/usr/bin/env python3
"""Verify native overview hover and the actual Nacre title in a private nested session."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import select
import shutil
import socket
import subprocess
import tempfile
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
PROTOCOL_URL = "https://raw.githubusercontent.com/swaywm/wlr-protocols/b010a03648b88d143236de193bddbfea0c08bc84/unstable/wlr-virtual-pointer-unstable-v1.xml"
PROTOCOL_SHA = "3ff6d540be0bc5228195bf072bde42117ea17945a5c2061add5d3cf97d6bb524"
POINTER_SOURCE = '#include <stdint.h>\n#include <stdlib.h>\n#include <string.h>\n#include <wayland-client.h>\n#include "virtual-pointer.h"\nstatic struct zwlr_virtual_pointer_manager_v1 *manager;\nstatic void global(void *data, struct wl_registry *registry, uint32_t name, const char *interface, uint32_t version) {\n    if (strcmp(interface, "zwlr_virtual_pointer_manager_v1") == 0)\n        manager = wl_registry_bind(registry, name, &zwlr_virtual_pointer_manager_v1_interface, version < 2 ? version : 2);\n}\nstatic void removed(void *data, struct wl_registry *registry, uint32_t name) {}\nstatic const struct wl_registry_listener listener = {global, removed};\nint main(int argc, char **argv) {\n    if (argc != 5) return 2;\n    struct wl_display *display = wl_display_connect(NULL);\n    if (!display) return 3;\n    struct wl_registry *registry = wl_display_get_registry(display);\n    wl_registry_add_listener(registry, &listener, NULL);\n    wl_display_roundtrip(display);\n    if (!manager) return 4;\n    struct zwlr_virtual_pointer_v1 *pointer = zwlr_virtual_pointer_manager_v1_create_virtual_pointer(manager, NULL);\n    zwlr_virtual_pointer_v1_motion_absolute(pointer, 1, atoi(argv[1]), atoi(argv[2]), atoi(argv[3]), atoi(argv[4]));\n    zwlr_virtual_pointer_v1_frame(pointer);\n    wl_display_roundtrip(display);\n    zwlr_virtual_pointer_v1_destroy(pointer);\n    wl_display_roundtrip(display);\n    wl_display_disconnect(display);\n    return 0;\n}\n'


def build_pointer(root):
    with urllib.request.urlopen(PROTOCOL_URL, timeout=20) as response:
        data = response.read()
    if hashlib.sha256(data).hexdigest() != PROTOCOL_SHA:
        raise ValueError("Virtual pointer protocol checksum mismatch")
    xml = root / "virtual-pointer.xml"
    xml.write_bytes(data)
    subprocess.run(
        ["wayland-scanner", "client-header", str(xml), str(root / "virtual-pointer.h")],
        check=True,
    )
    subprocess.run(
        ["wayland-scanner", "private-code", str(xml), str(root / "virtual-pointer.c")],
        check=True,
    )
    source = root / "move-pointer.c"
    source.write_text(POINTER_SOURCE)
    flags = subprocess.check_output(
        ["pkg-config", "--cflags", "--libs", "wayland-client"], text=True
    ).split()
    binary = root / "move-pointer"
    subprocess.run(
        [
            "gcc",
            "-O2",
            "-o",
            str(binary),
            str(source),
            str(root / "virtual-pointer.c"),
            *flags,
        ],
        check=True,
    )
    return binary


def verify(binary, output):
    binary = Path(binary).resolve()
    output = Path(output).resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    outer_runtime = Path(os.environ["XDG_RUNTIME_DIR"])
    outer_display = Path(os.environ["WAYLAND_DISPLAY"])
    if not outer_display.is_absolute():
        outer_display = outer_runtime / outer_display
    with tempfile.TemporaryDirectory(prefix="nhover-") as temporary:
        root = Path(temporary)
        runtime = root / "runtime"
        runtime.mkdir(mode=0o700)
        pointer = build_pointer(root)
        config = root / "config.kdl"
        config.write_text(
            'input { keyboard { xkb { layout "us"; }; }; }\nlayout { gaps 10; default-column-width { proportion 0.5; }; }\nhotkey-overlay { skip-at-startup; }\n'
        )
        env = dict(
            os.environ,
            XDG_RUNTIME_DIR=str(runtime),
            XDG_CONFIG_HOME=str(root / "home/.config"),
            XDG_DATA_HOME=str(root / "home/.local/share"),
            XDG_STATE_HOME=str(root / "home/.local/state"),
            XDG_CACHE_HOME=str(root / "home/.cache"),
            WAYLAND_DISPLAY=str(outer_display),
            HOME=str(root / "home"),
            NIRI_CONFIG=str(config),
            NACRE_COMPOSITOR="niri",
            QT_QPA_PLATFORM="wayland",
        )
        for key in ("NIRI_SOCKET", "DISPLAY", "NACRE_QS_CONFIG", "QT_LOGGING_RULES"):
            env.pop(key, None)
        processes = []
        with (output.with_suffix(".log")).open("wb") as log:
            # A private bus without activation directories keeps portal helpers
            # and the real desktop's user services out of this probe.
            bus_config = root / "bus.conf"
            bus_config.write_text("""<busconfig>
<type>session</type><listen>unix:tmpdir=/tmp</listen>
<policy context="default"><allow send_destination="*"/>
<allow receive_sender="*"/><allow own="*"/></policy>
</busconfig>""")
            bus = subprocess.Popen(
                [
                    "dbus-daemon",
                    "--nofork",
                    "--config-file",
                    str(bus_config),
                    "--print-address=1",
                ],
                stdout=subprocess.PIPE,
                stderr=log,
                text=True,
            )
            processes.append(bus)
            env["DBUS_SESSION_BUS_ADDRESS"] = bus.stdout.readline().strip()
            if not env["DBUS_SESSION_BUS_ADDRESS"]:
                raise RuntimeError("Private probe bus failed to start")
            compositor = subprocess.Popen(
                [str(binary), "--config", str(config)], env=env, stdout=log, stderr=log
            )
            processes.append(compositor)
            try:
                sockets = []
                for _ in range(100):
                    sockets = list(runtime.glob(f"niri.*.{compositor.pid}.sock"))
                    if sockets:
                        break
                    if compositor.poll() is not None:
                        raise RuntimeError(
                            "Nested compositor exited: "
                            + (output.with_suffix(".log")).read_text()[-2000:]
                        )
                    time.sleep(0.1)
                if not sockets:
                    raise RuntimeError("Nested IPC socket missing")
                env["NIRI_SOCKET"] = str(sockets[0])
                env["WAYLAND_DISPLAY"] = str(
                    next(
                        p
                        for p in runtime.glob("wayland-*")
                        if not p.name.endswith(".lock")
                    )
                )

                def msg(*args):
                    return subprocess.check_output(
                        [str(binary), "msg", *args], env=env, text=True
                    )

                for title in ["Hover Alpha", "Hover Beta"]:
                    processes.append(
                        subprocess.Popen(
                            [
                                "kitty",
                                "--config",
                                "NONE",
                                "--class",
                                "nacre-hover-probe",
                                "--title",
                                title,
                                "--",
                                "python3",
                                "-c",
                                "import time;time.sleep(120)",
                            ],
                            env=env,
                            stdout=log,
                            stderr=log,
                        )
                    )
                for _ in range(50):
                    windows = json.loads(msg("-j", "windows"))
                    if len(windows) == 2:
                        break
                    time.sleep(0.1)
                if len(windows) != 2:
                    raise RuntimeError("Probe windows missing")

                def subscribe(request):
                    s = socket.socket(socket.AF_UNIX)
                    s.connect(env["NIRI_SOCKET"])
                    s.sendall((json.dumps(request) + "\n").encode())
                    f = s.makefile("rb")
                    assert json.loads(f.readline()) == {"Ok": "Handled"}
                    s.setblocking(False)
                    return s, f

                legacy, legacy_file = subscribe("EventStream")
                hover, hover_file = subscribe("NacreOverviewHoverStream")
                # Nonblocking recv avoids a buffered makefile hiding ready messages.
                buffers = {legacy: b"", hover: b""}
                events = {legacy: [], hover: []}

                def drain(duration=0.1):
                    until = time.monotonic() + duration
                    while time.monotonic() < until:
                        ready, _, _ = select.select(
                            [legacy, hover], [], [], max(0, until - time.monotonic())
                        )
                        for s in ready:
                            chunk = s.recv(65536)
                            if not chunk:
                                raise RuntimeError("Stream disconnected")
                            buffers[s] += chunk
                            while b"\n" in buffers[s]:
                                line, buffers[s] = buffers[s].split(b"\n", 1)
                                events[s].append(json.loads(line))

                repo = ROOT
                qml = root / "bar"
                qml.mkdir()
                services = qml / "services"
                services.mkdir()
                shutil.copy2(
                    repo / "nacre/shell/services/NacreCompositor.qml",
                    services / "NacreCompositor.qml",
                )
                shutil.copytree(
                    repo / "nacre/shell/services/compositor", services / "compositor"
                )
                mocks = {
                    "NacreColours": 'property var palette:({m3primary:"#ffffff"})',
                    "NacreAudio": "property real volume:0.5;function setVolume(value){}",
                    "NacreI18n": "function tr(id,fallback){return fallback}function windowCaption(value,kind){return value}",
                    "ChatWindowTitle": "property var titles:({})",
                }
                manifest = "singleton NacreCompositor 1.0 NacreCompositor.qml\n"
                for name, body in mocks.items():
                    (services / (name + ".qml")).write_text(
                        "pragma Singleton\nimport QtQuick\nQtObject {" + body + "}\n"
                    )
                    manifest += "singleton " + name + " 1.0 " + name + ".qml\n"
                (services / "qmldir").write_text(manifest)
                for area, name, body in [
                    (
                        "config",
                        "NacreAppearance",
                        'property var spacing:({small:8});property var font:({family:{mono:"monospace"},size:{smaller:12}})',
                    ),
                    (
                        "utils",
                        "NacreIcons",
                        "function getAppCategoryIcon(name,fallback){return fallback}",
                    ),
                    (
                        "widgets",
                        "NacreTokens",
                        'property int textDuration:0;property bool motionEnabled:false;property color ink:"white";property string textFamily:"monospace";property real textPointSize:12',
                    ),
                ]:
                    folder = qml / area
                    folder.mkdir(exist_ok=True)
                    (folder / (name + ".qml")).write_text(
                        "pragma Singleton\nimport QtQuick\nQtObject {" + body + "}\n"
                    )
                    (folder / "qmldir").write_text(
                        "singleton " + name + " 1.0 " + name + ".qml\n"
                    )
                for name in ["NacreText", "NacreIcon"]:
                    shutil.copy2(
                        repo / ("nacre/shell/widgets/" + name + ".qml"),
                        qml / "widgets" / (name + ".qml"),
                    )
                    with (qml / "widgets/qmldir").open("a") as f:
                        f.write(name + " 1.0 " + name + ".qml\n")
                folder = qml / "modules/bar/components"
                folder.mkdir(parents=True)
                shutil.copy2(
                    repo / "nacre/shell/modules/bar/components/NacreActiveTitle.qml",
                    folder / "NacreActiveTitle.qml",
                )
                folder = qml / "utils/scripts"
                folder.mkdir()
                shutil.copy2(
                    repo / "nacre/shell/utils/scripts/window-title.js",
                    folder / "window-title.js",
                )
                (qml / "shell.qml").write_text(
                    'import QtQuick\nimport Quickshell\nimport Quickshell.Wayland\nimport "modules/bar/components"\nShellRoot {\n PanelWindow {\n  anchors {top:true;left:true;right:true}\n  implicitHeight:36;color:"#151515"\n  WlrLayershell.layer:WlrLayer.Overlay\n  WlrLayershell.keyboardFocus:WlrKeyboardFocus.None\n  exclusionMode:ExclusionMode.Ignore\n  mask:Region {}\n  NacreActiveTitle {anchors.fill:parent;monitor:null}\n }\n}\n'
                )
                bar_env = dict(
                    env,
                    NACRE_NIRI_HOVER_STREAM="1",
                    NACRE_COMPOSITOR_TOOLS=str(repo / "nacre/shell-tools"),
                )
                processes.append(
                    subprocess.Popen(
                        ["quickshell", "-p", str(qml), "-n"],
                        env=bar_env,
                        stdout=log,
                        stderr=log,
                    )
                )

                def bar_state():
                    return json.loads(
                        subprocess.check_output(
                            [
                                "quickshell",
                                "-p",
                                str(qml),
                                "ipc",
                                "call",
                                "windowTitle-default",
                                "state",
                            ],
                            env=bar_env,
                            text=True,
                            stderr=subprocess.DEVNULL,
                        )
                    )

                qml_captions = {}

                msg("action", "open-overview")
                time.sleep(0.4)
                drain(0.1)
                focus_in_overview = json.loads(msg("-j", "focused-window"))
                selection_in_overview = {
                    w["id"]: w["active_window_id"]
                    for w in json.loads(msg("-j", "workspaces"))
                }
                outputs = json.loads(msg("-j", "outputs"))
                out = next(iter(outputs.values()))
                width = out["logical"]["width"]
                height = out["logical"]["height"]
                found = {}
                latencies = []
                for y in range(height // 8, height, height // 8):
                    for x in range(width // 8, width, width // 8):
                        started = time.monotonic()
                        subprocess.run(
                            [str(pointer), str(x), str(y), str(width), str(height)],
                            env=env,
                            check=True,
                        )
                        drain(0.02)
                        under = json.loads(msg("-j", "window-under-cursor"))
                        if under and under["id"] not in found:
                            found[under["id"]] = [x, y, under["title"]]
                            latencies.append((time.monotonic() - started) * 1000)
                        if len(found) == 2:
                            break
                    if len(found) == 2:
                        break
                assert len(found) == 2, (
                    f"Could not hit both preview windows: {found} {width}x{height}"
                )

                for identifier, point in found.items():
                    subprocess.run(
                        [
                            str(pointer),
                            str(point[0]),
                            str(point[1]),
                            str(width),
                            str(height),
                        ],
                        env=env,
                        check=True,
                    )
                    for _ in range(30):
                        try:
                            caption = bar_state()["title"]
                            if caption == point[2]:
                                break
                        except (subprocess.SubprocessError, ValueError):
                            pass
                        time.sleep(0.05)
                    assert caption == point[2], (
                        f"QML title does not follow hover: {caption} != {point[2]}"
                    )
                    qml_captions[identifier] = caption

                assert json.loads(msg("-j", "focused-window")) == focus_in_overview, (
                    "Hover changed keyboard focus"
                )
                assert {
                    w["id"]: w["active_window_id"]
                    for w in json.loads(msg("-j", "workspaces"))
                } == selection_in_overview, "Hover changed selected windows"
                hover_ids = [
                    e["NacreOverviewHoverChanged"]["id"]
                    for e in events[hover]
                    if "NacreOverviewHoverChanged" in e
                ]
                assert set(found) <= set(hover_ids), (
                    f"Missing hover events {found} {hover_ids}"
                )
                assert not any(
                    "NacreOverviewHoverChanged" in e for e in events[legacy]
                ), "Hover leaked into legacy stream"
                msg("action", "close-overview")
                drain(0.1)
                assert [
                    e["NacreOverviewHoverChanged"]["id"]
                    for e in events[hover]
                    if "NacreOverviewHoverChanged" in e
                ][-1] is None
                report = {
                    "sourceRevision": "8ed0da44d974c32c6877d2f4630c314da0717ecb",
                    "windows": found,
                    "focusPreserved": True,
                    "legacyStreamUnaffected": True,
                    "hoverClearedOnClose": True,
                    "probeRoundtripMs": latencies,
                    "nestedVerified": True,
                    "qmlCaptionVerified": True,
                    "qmlCaptions": qml_captions,
                    "binarySha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                    "patchSha256": hashlib.sha256(
                        (
                            repo / "tools/patches/niri-26.04-overview-hover.patch"
                        ).read_bytes()
                    ).hexdigest(),
                }
                output.write_text(json.dumps(report, indent=2))
                print(json.dumps(report))
                hover_file.close()
                legacy_file.close()
                hover.close()
                legacy.close()
            finally:
                for process in reversed(processes):
                    if process.poll() is None:
                        process.terminate()
                for process in processes:
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    verify(args.binary, args.output)


if __name__ == "__main__":
    main()
