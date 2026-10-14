#!/usr/bin/env python3
"""Run the Settings review prototype in a private nested Niri session only."""

import argparse
import json
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

from settings_preview_fixtures import stage, state


def wait_for(read, accept=bool, timeout=15):
    deadline = time.monotonic() + timeout
    last = None
    while time.monotonic() < deadline:
        last = read()
        if accept(last):
            return last
        time.sleep(0.08)
    raise RuntimeError("Preview timed out: " + str(last))


def run(output, gallery, keep, production=False):
    for program in ["niri", "quickshell", "grim"]:
        if not shutil.which(program):
            raise RuntimeError("Preview requires " + program)
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="nacre-settings-preview-") as directory:
        root = Path(directory)
        config = stage(root / "shell", icons=True)
        if production:
            shutil.rmtree(config)
            shutil.copytree(Path(__file__).resolve().parents[1] / "nacre/shell", config)
            (config / "shell.qml").write_text("""import Quickshell
import Quickshell.Io
import qs.modules.settings
import qs.services
ShellRoot {
 NacreSettingsHost {}
 property var powerPolicy:NacrePowerPolicy
 IpcHandler {target:"settingsRuntime"
  function write(id:string,value:string):bool {return NacreSettingsProvider.write({id:id},JSON.parse(value))}
  function state():string {return JSON.stringify({files:NacreSettingsProvider.files,error:NacreSettingsProvider.error})}
 }
}
""")
        runtime = root / "runtime"
        runtime.mkdir(mode=0o700)
        home = root / "home"
        home.mkdir()
        seed = home / "preview-state.json"
        seed.write_text(json.dumps(state()))
        # Connect the nested compositor to the parent presentation socket only.
        # All child IPC, configuration, cache, state and shell sessions are private.
        parent = Path(os.environ["WAYLAND_DISPLAY"])
        if not parent.is_absolute():
            parent = Path(os.environ["XDG_RUNTIME_DIR"]) / parent
        env = dict(os.environ)
        for key in [
            "NIRI_SOCKET",
            "HYPRLAND_INSTANCE_SIGNATURE",
            "NOTIFY_SOCKET",
            "NACRE_QS_CONFIG",
            "DISPLAY",
            "NIRI_CONFIG",
            "QT_LOGGING_RULES",
            *[key for key in env if key.startswith("NACRE_")],
        ]:
            env.pop(key, None)
        env.update(
            HOME=str(home),
            XDG_RUNTIME_DIR=str(runtime),
            XDG_CONFIG_HOME=str(home / ".config"),
            XDG_CACHE_HOME=str(home / ".cache"),
            XDG_STATE_HOME=str(home / ".local/state"),
            XDG_DATA_HOME=str(home / ".local/share"),
            WAYLAND_DISPLAY=str(parent),
            NACRE_SETTINGS_PREVIEW_STATE=str(seed),
            NACRE_SETTINGS_PREVIEW_ICON_FONT=str(config / "MaterialSymbolsRounded.ttf"),
            QT_QUICK_CONTROLS_STYLE="Basic",
            QT_NO_XDG_DESKTOP_PORTAL="1",
            DBUS_SESSION_BUS_ADDRESS="unix:path=" + str(runtime / "bus"),
            DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + str(runtime / "bus"),
        )
        if production:
            source_root = Path(__file__).resolve().parents[1]
            tools = home / ".local/share/nacre/shell/tools"
            shutil.copytree(source_root / "nacre/shell-tools", tools)
            helper = Path(
                os.environ.get(
                    "NACRE_HELPER_BINARY", source_root / "target/release/nacre-helper"
                )
            )
            if not helper.is_file():
                raise RuntimeError("Production smoke needs the task native helper")
            shutil.copy2(helper, tools / "nacre-helper")
            config_dir = home / ".config/nacre"
            config_dir.mkdir(parents=True)
            shutil.copy2(
                source_root / "profiles/default.json", config_dir / "profile.json"
            )
            env.update(NACRE_COMPOSITOR="niri")
        niri_config = root / "config.kdl"
        niri_config.write_text("""
output "winit" {mode "1440x1000";}
layout {gaps 24; background-color "#0c151b";}
hotkey-overlay {skip-at-startup;}
window-rule {match title="^Nacre Settings Preview$";open-floating true;}
""")
        env["NIRI_CONFIG"] = str(niri_config)
        bus_config = root / "bus.conf"
        bus_config.write_text(
            "<busconfig><type>session</type><listen>unix:path="
            + str(runtime / "bus")
            + '</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>'
        )
        processes = []
        with (output / "nested.log").open("w") as log:
            try:
                bus = subprocess.Popen(
                    ["dbus-daemon", "--nofork", "--config-file=" + str(bus_config)],
                    env=env,
                    stdout=log,
                    stderr=log,
                )
                processes.append(bus)
                wait_for(lambda: (runtime / "bus").exists())
                compositor = subprocess.Popen(
                    ["nice", "-n", "10", "/usr/bin/niri", "-c", str(niri_config)],
                    env=env,
                    stdout=log,
                    stderr=log,
                )
                processes.append(compositor)
                socket = wait_for(
                    lambda: list(runtime.glob(f"niri.*.{compositor.pid}.sock"))
                )[0]
                wayland = wait_for(
                    lambda: [p for p in runtime.glob("wayland-*") if p.is_socket()]
                )[0]
                env.update(NIRI_SOCKET=str(socket), WAYLAND_DISPLAY=str(wayland))
                shell = subprocess.Popen(
                    [
                        "nice",
                        "-n",
                        "10",
                        "quickshell",
                        "-p",
                        str(config / "shell.qml"),
                        "--no-duplicate",
                    ],
                    env=env,
                    stdout=log,
                    stderr=log,
                )
                processes.append(shell)

                def ipc(method, *args):
                    return subprocess.run(
                        [
                            "quickshell",
                            "ipc",
                            "-p",
                            str(config / "shell.qml"),
                            "call",
                            "settingsApp" if production else "settingsPreview",
                            method,
                            *map(str, args),
                        ],
                        env=env,
                        capture_output=True,
                        text=True,
                        timeout=10 if production else 4,
                    )

                def snapshot():
                    try:
                        result = ipc("state")
                    except subprocess.TimeoutExpired:
                        return {}
                    return json.loads(result.stdout) if result.returncode == 0 else {}

                if production:
                    wait_for(snapshot, lambda value: "created" in value, timeout=30)
                    ipc("open", "panels").check_returncode()
                    wait_for(
                        snapshot,
                        lambda value: (
                            value.get("visible") and value.get("width", 0) > 0
                        ),
                    )
                    checked = []
                    for route in [
                        "wallpaper",
                        "colors",
                        "appearance",
                        "panels",
                        "launcher",
                        "windows-workspaces",
                        "notifications",
                        "displays",
                        "sound",
                        "network",
                        "keyboard-mouse",
                        "lock-login",
                        "apps",
                        "privacy",
                        "date-time-language",
                        "system",
                        "addons",
                        "panels/dashboard#pages",
                        "panels/control-center#tiles",
                        "colors/app-theming",
                        "launcher/favorites",
                        "launcher/entries",
                        "workflows",
                        "time",
                        "lock",
                        "maintenance",
                        "ai",
                    ]:
                        ipc("open", route).check_returncode()
                        time.sleep(0.15)
                        checked.append({"route": route, "state": snapshot()})
                    native_write = subprocess.run(
                        [
                            "quickshell",
                            "ipc",
                            "-p",
                            str(config / "shell.qml"),
                            "call",
                            "settingsRuntime",
                            "write",
                            "launcher.sources.files",
                            "false",
                        ],
                        env=env,
                        capture_output=True,
                        text=True,
                        timeout=4,
                    )
                    native_write.check_returncode()
                    if "true" not in native_write.stdout:
                        raise RuntimeError("Production store rejected launcher source")
                    sources = home / ".config/nacre/launcher-sources.json"
                    wait_for(
                        lambda: (
                            json.loads(sources.read_text()) if sources.exists() else {}
                        ),
                        lambda value: value.get("files") is False,
                    )
                    # Exercise the production descriptor/store/provider path in
                    # this compositor's isolated preferences and configuration.
                    result = subprocess.run(
                        [
                            "quickshell",
                            "ipc",
                            "-p",
                            str(config / "shell.qml"),
                            "call",
                            "desktopSettings",
                            "set",
                            "lipHoverDelay",
                            "420",
                        ],
                        env=env,
                        capture_output=True,
                        text=True,
                        timeout=4,
                    )
                    result.check_returncode()
                    saved = home / ".config/nacre/desktop.json"
                    wait_for(
                        lambda: json.loads(saved.read_text()) if saved.exists() else {},
                        lambda value: value.get("lipHoverDelay") == 420,
                    )
                    ipc("open", "panels#hoverDelay").check_returncode()
                    time.sleep(0.3)
                    (output / "production-smoke.json").write_text(
                        json.dumps(
                            {
                                "routes": checked,
                                "privateStoreAcknowledged": True,
                                "productionProviderAcknowledged": True,
                                "desktopVersion": json.loads(saved.read_text())[
                                    "version"
                                ],
                            },
                            indent=2,
                        )
                        + "\n"
                    )
                    if "WARN scene" in (output / "nested.log").read_text():
                        raise RuntimeError(
                            "Production Settings emitted scene warnings; inspect nested.log"
                        )
                    return
                initial = wait_for(snapshot, lambda value: value.get("writable"))
                if initial.get("systemBus") != env["DBUS_SYSTEM_BUS_ADDRESS"]:
                    raise RuntimeError(
                        "Preview system bus is not isolated: "
                        + str(initial.get("systemBus"))
                    )
                report = {
                    "version": 1,
                    "baseRevision": subprocess.check_output(
                        ["git", "rev-parse", "HEAD"], text=True
                    ).strip(),
                    "fixture": "Synthetic desktop; AI/Brain/battery/Wi-Fi/Bluetooth available; packages absent; planned backends disabled",
                    "surfaces": [],
                }
                source_root = Path(__file__).resolve().parents[1]
                originals = [
                    path
                    for path in (source_root / "nacre/shell").rglob("*")
                    if path.is_file() and path.suffix in [".qml", ".js", ".json"]
                ]
                originals += [
                    source_root / "nacre/shell-tools/theme-targets.json",
                    source_root / "nacre/shell-tools/nacre-state-files.json",
                    Path(__file__),
                    Path(__file__).with_name("settings_preview_fixtures.py"),
                ]
                report["sourceHashes"] = {
                    str(path.relative_to(source_root)): hashlib.sha256(
                        path.read_bytes()
                    ).hexdigest()
                    for path in originals
                }
                report["capture"] = (
                    "Native Qt grabToImage on the rendered window surface inside nested Niri"
                )
                if gallery:
                    # Prove actual native writes and file-change acknowledgements
                    # before capturing; then restore the initial review choices.
                    assert (
                        "false"
                        in ipc(
                            "write", "panels.lips.clickEdgeMenus", '"invalid"'
                        ).stdout
                    )
                    assert (
                        "true"
                        in ipc("write", "panels.lips.clickEdgeMenus", "true").stdout
                    )
                    wait_for(
                        snapshot,
                        lambda value: (
                            value.get("files", {})
                            .get("desktop", {})
                            .get("clickEdgeMenus")
                            is True
                        ),
                    )
                    assert (
                        json.loads(seed.read_text())["files"]["desktop"][
                            "clickEdgeMenus"
                        ]
                        is True
                    )
                    assert (
                        "true"
                        in ipc("write", "panels.lips.clickEdgeMenus", "false").stdout
                    )
                    wait_for(
                        snapshot,
                        lambda value: (
                            value.get("files", {})
                            .get("desktop", {})
                            .get("clickEdgeMenus")
                            is False
                        ),
                    )
                    assert (
                        "true" in ipc("write", "colors.palette.mode", '"light"').stdout
                    )
                    wait_for(
                        snapshot,
                        lambda value: (
                            value.get("files", {})
                            .get("colors", {})
                            .get("ui", {})
                            .get("modeChoice")
                            == "light"
                        ),
                    )
                    assert (
                        "true" in ipc("write", "colors.palette.mode", '"dark"').stdout
                    )
                    wait_for(
                        snapshot,
                        lambda value: (
                            value.get("files", {})
                            .get("colors", {})
                            .get("ui", {})
                            .get("modeChoice")
                            == "dark"
                        ),
                    )
                    report["persistence"] = (
                        "Native click/hover and light/dark writes acknowledged and stored; choices restored; invalid type rejected"
                    )
                    ipc("development", "false").check_returncode()
                    for width, height, label in [
                        (1100, 840, "normal"),
                        (620, 840, "narrow"),
                    ]:
                        ipc("size", width, height).check_returncode()
                        windows = json.loads(
                            subprocess.check_output(
                                ["/usr/bin/niri", "msg", "-j", "windows"],
                                env=env,
                                text=True,
                            )
                        )
                        window = next(
                            item
                            for item in windows
                            if item["title"] == "Nacre Settings Preview"
                        )
                        for axis, value in [("width", width), ("height", height)]:
                            subprocess.run(
                                [
                                    "/usr/bin/niri",
                                    "msg",
                                    "action",
                                    "set-window-" + axis,
                                    "--id",
                                    str(window["id"]),
                                    str(value),
                                ],
                                env=env,
                                capture_output=True,
                                check=True,
                            )
                        wait_for(
                            snapshot,
                            lambda value, width=width, height=height: (
                                value.get("width") == width
                                and value.get("height") == height
                            ),
                        )
                        routes = [
                            page["id"]
                            for fragment in json.loads(
                                (
                                    config / "modules/settings/registry/manifest.json"
                                ).read_text()
                            )["fragments"]
                            for page in json.loads(
                                (
                                    config / "modules/settings/registry" / fragment
                                ).read_text()
                            ).get("pages", [])
                            if page["id"] != "addons"
                        ] + ["search", "highlight"]
                        for route in routes:
                            if route == "search":
                                ipc("search", "palette").check_returncode()
                            else:
                                target = (
                                    "panels#clickEdgeMenus"
                                    if route == "highlight"
                                    else route
                                )
                                result = ipc("open", target)
                                result.check_returncode()
                                if "true" not in result.stdout:
                                    raise RuntimeError("Rejected route: " + target)
                            time.sleep(0.35)
                            current = snapshot()
                            filename = label + "-" + route.replace("/", "-") + ".png"
                            destination = output / filename
                            destination.unlink(missing_ok=True)
                            ipc("capture", destination).check_returncode()
                            wait_for(
                                lambda destination=destination: destination.exists()
                            )
                            current = snapshot()
                            if current["width"] != width or current["height"] != height:
                                raise RuntimeError(
                                    "Compositor did not resize preview: " + str(current)
                                )
                            report["surfaces"].append(
                                {
                                    "file": filename,
                                    "state": {
                                        key: current.get(key)
                                        for key in [
                                            "page",
                                            "query",
                                            "highlight",
                                            "width",
                                            "height",
                                            "error",
                                        ]
                                    },
                                }
                            )
                            if route in ["panels", "colors", "launcher", "power"]:
                                ipc("scroll", "1").check_returncode()
                                lower = label + "-" + route + "-lower.png"
                                lower_path = output / lower
                                lower_path.unlink(missing_ok=True)
                                ipc("capture", lower_path).check_returncode()
                                wait_for(
                                    lambda lower_path=lower_path: lower_path.exists()
                                )
                                report["surfaces"].append(
                                    {
                                        "file": lower,
                                        "state": {
                                            "page": route,
                                            "width": width,
                                            "height": height,
                                            "scroll": "bottom",
                                        },
                                    }
                                )
                                ipc("scroll", "0").check_returncode()
                    # Gating checks use this preview's IPC, never the live shell.
                    ipc("capability", "battery", "false").check_returncode()
                    assert "false" in ipc("open", "power").stdout
                    ipc("capability", "ai", "false").check_returncode()
                    assert "false" in ipc("open", "panels/ai-sidebar").stdout
                    assert "true" in ipc("open", "ai").stdout
                    assert snapshot()["page"] == "addons"
                    report["capabilityGates"] = (
                        "battery/AI routes rejected; legacy AI routes to Add-ons"
                    )
                    (output / "gallery.json").write_text(
                        json.dumps(report, indent=2) + "\n"
                    )
                    shutil.copy2(seed, output / "preview-state.json")
                if keep or not gallery:
                    print(
                        "Nested review session running; close its Settings window to finish.",
                        flush=True,
                    )
                    shell.wait()
            finally:
                for process in reversed(processes):
                    if process.poll() is None:
                        process.terminate()
                for process in reversed(processes):
                    try:
                        process.wait(timeout=3)
                    except subprocess.TimeoutExpired:
                        process.kill()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output", type=Path, default=Path("/tmp/nacre-settings-gallery")
    )
    parser.add_argument("--gallery", action="store_true")
    parser.add_argument("--keep", action="store_true")
    parser.add_argument(
        "--production",
        action="store_true",
        help="Smoke the complete production shell and providers in private nested state",
    )
    args = parser.parse_args()
    run(args.output.resolve(), args.gallery, args.keep, args.production)


if __name__ == "__main__":
    main()
