#!/usr/bin/env python3
"""Measure the production shell in a disposable headless/nested Wayland session."""

import argparse
import collections
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import tarfile
import io
import hashlib
import importlib.util
import time

import performance

ROOT = Path(__file__).resolve().parent.parent

# Instrument only the disposable copy. F8/F9 are delivered by a virtual keyboard;
# the clock starts at Qt's key handler, not at wtype/IPC process creation.
PROBE = """
    property int perfSequence: 0
    property real perfStarted: 0
    property string perfAction: ""
    property var perfResult: ({})
    property var perfWait: ({})
    function perfOpen(action) {
        perfSequence++;
        perfAction = action;
        perfResult = ({});
        perfStarted = Date.now();
        if (action === "launcher") NacrePanelState.openMode("apps", "", false);
        else { NacrePanelState.openEdge("dashboard", root.screen.name); flags.dashboardTab = 0; flags.dashboardPinned = true; }
        console.log("PERF_KEY", action, perfSequence);
        perfResult = {sequence: perfSequence, synchronousMs: Date.now() - perfStarted};
    }
    function perfFind(item, name) {
        if (item.objectName === name) return item;
        for (const child of item.children || []) { const found = perfFind(child, name); if (found) return found; }
        return null;
    }
    function perfImages(item) {
        let ready = 0, pending = 0;
        if (item.source !== undefined && item.status !== undefined && String(item.source)) {
            if (item.status === 1) ready++;
            if (item.status === 2) pending++;
        }
        for (const child of item.children || []) {
            const counts = perfImages(child); ready += counts.ready; pending += counts.pending;
        }
        return {ready: ready, pending: pending};
    }
    Connections {
        target: root.contentItem.Window.window
        function onFrameSwapped() {
            if (!root.perfStarted) return;
            const panel = root.perfAction === "launcher" ? panelHost.launcher : panelHost.dashboard;
            root.perfWait = {visible: panel.visible, opening: panel.opening, presented: panel.presentedHeight, content: panel.contentHeight, queue: NacreDashboardState.queue.length, message: NacreDashboardState.message, images: root.perfImages(panel)};
            if (root.perfFind(panel, root.perfAction === "launcher" ? "launcherPage" : "dashboardPage")?.status !== Loader.Ready) return;
            if (!panel.visible || panel.opening || Math.abs(panel.presentedHeight - panel.contentHeight) > 1) return;
            if (root.perfAction === "dashboard" && (NacreDashboardState.busy || NacreDashboardState.queue.length || NacreDashboardState.message)) return;
            const images = root.perfImages(panel);
            if (images.pending) return;
            root.perfResult = Object.assign({}, root.perfResult, {frameMs: Date.now() - root.perfStarted, readyImages: images.ready});
            root.perfStarted = 0;
        }
    }
    Timer { interval: 16; repeat: true; running: root.perfStarted > 0; onTriggered: root.contentItem.Window.window.update(); }
    FloatingWindow {
        id: perfInput
        title: "Nacre Performance Input"
        visible: true
        implicitWidth: 64
        implicitHeight: 48
        Item {
            id: perfKeys
            anchors.fill: parent
            focus: true
            Keys.onPressed: event => {
                if (event.key === Qt.Key_F8 || event.key === Qt.Key_F9) {
                    root.perfOpen(event.key === Qt.Key_F8 ? "launcher" : "dashboard");
                    event.accepted = true;
                }
            }
        }
    }
    IpcHandler {
        target: "performanceFrame"
        function state(): string { return JSON.stringify(Object.assign({}, root.perfResult, {action: root.perfAction, started: root.perfStarted, screen: root.screen.name, width: root.screen.width, height: root.screen.height, favorites: LauncherPreferences.favorites.length, visibleFavorites: NacreMenu.favorites().length, waiting: root.perfWait})); }
        function arm(): void { perfInput.visible = false; perfInput.visible = true; perfKeys.forceActiveFocus(); }
        function suspend(hidden: bool): void { WallpaperPlayback.locked = hidden; }
        function pinDashboard(): void { flags.dashboardPinned = true; }
        function pause(paused: bool): void { NacreWallpapers.preferences = Object.assign({}, NacreWallpapers.preferences, {paused: paused}); }
        function open(action: string): void { root.perfOpen(action); }
        function wallpaper(data: string): void {
            const fixture = JSON.parse(data);
            NacreWallpapers.media = fixture.media;
            NacreWallpapers.lastImage = fixture.media.poster;
            NacrePresentation.accept(fixture.presentation);
        }
    }
"""


def wait_for(predicate, description, timeout=20):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = predicate()
        if result:
            return result
        time.sleep(0.05)
    raise RuntimeError("Timed out: " + description)


def mapping(pid):
    groups = collections.defaultdict(float)
    group = "anonymous"
    for line in Path(f"/proc/{pid}/smaps").read_text().splitlines():
        if re.match(r"^[0-9a-f]+-[0-9a-f]+ ", line):
            parts = line.split(maxsplit=5)
            name = parts[5] if len(parts) > 5 else "[anonymous]"
            group = (
                "shared_buffers"
                if "memfd:" in name or "/dev/shm" in name
                else "anonymous"
                if name.startswith("[")
                else "fonts"
                if "/fonts/" in name
                else "multimedia"
                if any(
                    word in name
                    for word in ("libav", "libsw", "Multimedia", "multimedia")
                )
                else "qt_qml"
                if any(word in name for word in ("Qt", "qml"))
                else "mesa_llvm"
                if any(word in name for word in ("LLVM", "dri.so", "gallium"))
                else "other_files"
            )
        elif line.startswith("Pss:"):
            groups[group] += int(line.split()[1]) / 1024
    return dict(groups)


def fixture(home, source, video=False):
    poster = home / "Pictures/Wallpapers/static.png"
    media = {
        "path": str(source),
        "poster": str(poster),
        "name": "Demo",
        "dynamic": video,
        "animated": False,
    }
    colours = json.loads(
        re.search(
            r"const fallback = (\{.*?\});",
            (ROOT / "nacre/shell/services/colour-data.js").read_text(),
            re.S,
        )[1]
    )
    return {
        "media": media,
        "presentation": {
            "mode": "dark",
            "poster": str(poster),
            "colours": colours,
            "publishedAtMs": time.time() * 1000,
        },
    }


def prepare(directory, revision=None):
    from PIL import Image

    home = directory / "home"
    shell = directory / "shell"
    source = ROOT
    if revision:
        archive = subprocess.check_output(
            ["git", "archive", revision, "nacre/shell", "nacre/shell-tools"], cwd=ROOT
        )
        source = directory / "source"
        source.mkdir()
        with tarfile.open(fileobj=io.BytesIO(archive)) as files:
            files.extractall(source, filter="data")
    shutil.copytree(source / "nacre/shell", shell)
    tools = home / ".local/share/nacre/shell/tools"
    shutil.copytree(
        source / "nacre/shell-tools",
        tools,
        ignore=shutil.ignore_patterns("tests", "__pycache__"),
    )
    (tools / "branding").symlink_to(ROOT / "nacre/branding", target_is_directory=True)
    runtime_python = home / ".local/share/nacre/palette-runtime/venv"
    runtime_python.parent.mkdir(parents=True)
    runtime_python.symlink_to(sys.prefix, target_is_directory=True)
    for relative in (
        ".config/nacre",
        ".cache",
        ".local/state/nacre/wallpaper",
        ".local/share/applications",
        "Pictures/Wallpapers",
    ):
        (home / relative).mkdir(parents=True, exist_ok=True)
    Image.linear_gradient("L").resize((3840, 2160)).convert("RGB").save(
        home / "Pictures/Wallpapers/static.png"
    )
    movie = home / "Pictures/Wallpapers/video.mp4"
    subprocess.run(
        [
            "ffmpeg",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=1920x1080:rate=24",
            "-t",
            "3",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
            str(movie),
        ],
        check=True,
        capture_output=True,
    )
    config = home / ".config/nacre"
    (config / "wallpaper.json").write_text(
        json.dumps(
            {
                "rotationEnabled": False,
                "postHook": True,
                "directory": str(home / "Pictures/Wallpapers"),
            }
        )
    )
    (config / "welcome.json").write_text(
        json.dumps({"version": 1, "showAtLogin": False})
    )
    favorites = [f"performance-demo-{i:02d}" for i in range(12)]
    (config / "launcher.json").write_text(json.dumps({"favorites": favorites}))
    for i in range(48):
        icon = home / f".local/share/applications/demo-{i}.png"
        Image.new("RGB", (128, 128), (i * 5, 90, 150)).save(icon)
        (icon.parent / f"performance-demo-{i:02d}.desktop").write_text(
            f"[Desktop Entry]\nType=Application\nName=Demo {i:02d}\nExec=true\nIcon={icon}\nCategories=Utility;\n"
        )
    static = fixture(home, home / "Pictures/Wallpapers/static.png")
    state = home / ".local/state/nacre"
    (state / "presentation.json").write_text(json.dumps(static["presentation"]))
    (state / "wallpaper/media.json").write_text(json.dumps(static["media"]))
    (state / "wallpaper/path.txt").write_text(static["media"]["poster"])
    (state / "wallpaper/last.txt").write_text(static["media"]["poster"])
    decoder = shell / "modules/background/NacreDesktopVideo.qml"
    decoder_text = decoder.read_text().replace(
        "property bool frameReady: false",
        "property bool frameReady: false\n    property int measuredFrames: 0",
    )
    decoder_text = decoder_text.replace(
        "function onVideoFrameChanged() {",
        "function onVideoFrameChanged() {\n            root.measuredFrames++;",
    )
    decoder_text = decoder_text.replace(
        "firstFrame: root.frameReady,",
        "firstFrame: root.frameReady,\n                measuredFrames: root.measuredFrames,",
    )
    decoder.write_text(decoder_text)
    dashboard = shell / "services/NacreDashboardState.qml"
    if "readonly property bool busy:" not in dashboard.read_text():
        dashboard.write_text(
            dashboard.read_text().replace(
                "    id: root",
                "    id: root\n    readonly property bool busy: worker.running",
                1,
            )
        )
    screen = shell / "modules/drawers/NacreScreen.qml"
    text = screen.read_text().replace(
        "import QtQuick\n",
        "import QtQuick\nimport QtQuick.Window\nimport Quickshell.Io\n",
        1,
    )
    text = text.replace("    id: root\n", "    id: root\n" + PROBE, 1)
    screen.write_text(text)
    return home, shell, movie


def shell_environment(tools, environment):
    """Use the production supervisor's child policy without running its main loop."""
    previous = dict(os.environ)
    sys.path.insert(0, str(tools))
    try:
        os.environ.clear()
        os.environ.update(environment)
        spec = importlib.util.spec_from_file_location(
            "performance_supervisor", tools / "shell-supervisor.py"
        )
        supervisor = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(supervisor)
        return supervisor.environment()
    finally:
        os.environ.clear()
        os.environ.update(previous)
        sys.path.remove(str(tools))


def run(args, directory):
    home, shell, movie = prepare(directory, args.revision)
    # Unix-domain socket paths (including Hyprland signatures) must fit 108 bytes.
    runtime = Path(tempfile.mkdtemp(prefix="npr-"))
    env = {
        key: value
        for key, value in os.environ.items()
        if not key.startswith(("NACRE_", "QT_", "QSG_", "XDG_", "DBUS_"))
        and key
        not in (
            "WAYLAND_DISPLAY",
            "DISPLAY",
            "HYPRLAND_INSTANCE_SIGNATURE",
            "NIRI_SOCKET",
        )
    }
    env.update(
        HOME=str(home),
        XDG_RUNTIME_DIR=str(runtime),
        XDG_CONFIG_HOME=str(home / ".config"),
        XDG_CACHE_HOME=str(home / ".cache"),
        XDG_STATE_HOME=str(home / ".local/state"),
        XDG_DATA_HOME=str(home / ".local/share"),
        XDG_DATA_DIRS="/usr/local/share:/usr/share",
        QT_QPA_PLATFORM="wayland",
        QT_SCALE_FACTOR="1",
        NACRE_COMPOSITOR=args.backend,
        PYTHONPATH=str(ROOT / "nacre/shell-cli/src"),
    )
    if args.orient_binary:
        env["NACRE_ORIENT_BINARY"] = str(args.orient_binary.resolve(strict=True))
    processes = []

    def start(command, name, environment):
        log = (directory / (name + ".log")).open("w")
        process = subprocess.Popen(
            command, env=environment, stdout=log, stderr=log, start_new_session=True
        )
        log.close()
        processes.append(process)
        return process

    def ipc(*words):
        result = subprocess.run(
            ["quickshell", "ipc", "--pid", str(pid), "call", *words],
            env=env,
            capture_output=True,
            text=True,
            timeout=10,
        )
        if result.returncode:
            raise RuntimeError(result.stderr[-800:])
        return result.stdout.strip()

    def state(*words):
        try:
            return json.loads(ipc(*words))
        except (ValueError, RuntimeError):
            return None

    try:
        parent_env = dict(
            env,
            WLR_BACKENDS="headless",
            WLR_RENDERER="gles2",
            WLR_RENDERER_ALLOW_SOFTWARE="1",
            WLR_LIBINPUT_NO_DEVICES="1",
            WLR_HEADLESS_OUTPUTS="1",
        )
        if args.labwc_libraries:
            parent_env["LD_LIBRARY_PATH"] = str(args.labwc_libraries)
        empty = directory / "empty"
        empty.mkdir()
        (empty / "rc.xml").write_text(
            '<labwc_config><windowRules><windowRule identifier="niri"><action name="ToggleFullscreen"/></windowRule></windowRules></labwc_config>'
        )
        start([args.labwc, "-C", str(empty)], "parent", parent_env)
        wait_for(lambda: (runtime / "wayland-0").exists(), "headless parent")
        env["WAYLAND_DISPLAY"] = "wayland-0"
        subprocess.run(
            [args.wlr_randr, "--output", "HEADLESS-1", "--custom-mode", "1920x1080@60"],
            env=env,
            check=True,
            capture_output=True,
        )
        if args.backend == "hyprland":
            config = directory / "hyprland.lua"
            config.write_text(
                'hl.monitor({output="",mode="1920x1080@60",position="auto",scale=1})\nhl.window_rule({name="performance-input",match={class="^org[.]quickshell$"},float=true})\nhl.config({animations={enabled=false},misc={disable_hyprland_logo=true,disable_splash_rendering=true}})\n'
            )
            start(
                [
                    "Hyprland",
                    *(["--i-am-really-stupid"] if os.getuid() == 0 else []),
                    "-c",
                    str(config),
                ],
                "compositor",
                dict(env, AQ_DRM_DEVICES="/dev/null", HYPRLAND_NO_CRASHREPORTER="1"),
            )
            signature = wait_for(
                lambda: next(iter(runtime.glob("hypr/*/.socket.sock")), None),
                "Hyprland socket",
            ).parent.name
            env["HYPRLAND_INSTANCE_SIGNATURE"] = signature
            subprocess.run(
                ["hyprctl", "output", "create", "headless"],
                env=env,
                check=True,
                capture_output=True,
            )
        elif args.backend == "niri":
            config = directory / "niri.kdl"
            config.write_text(
                'input { keyboard { xkb { layout "us"; }; }; };\noutput "winit" { mode "1920x1080"; scale 1; };\nprefer-no-csd\nwindow-rule { match app-id="^org[.]quickshell$"; open-floating true; }\n'
            )
            start(["niri", "-c", str(config)], "compositor", env)
            socket = wait_for(
                lambda: next(iter(runtime.glob("niri.*.sock")), None), "niri socket"
            )
            env["NIRI_SOCKET"] = str(socket)
        if args.backend != "labwc":
            wait_for(lambda: (runtime / "wayland-1").exists(), "nested Wayland socket")
            env["WAYLAND_DISPLAY"] = "wayland-1"
        shell_env = shell_environment(home / ".local/share/nacre/shell/tools", env)
        if args.allocator_mmap_threshold:
            shell_env["MALLOC_MMAP_THRESHOLD_"] = str(args.allocator_mmap_threshold)
        process = start(
            ["dbus-run-session", "--", "quickshell", "-p", str(shell), "-n"],
            "shell",
            shell_env,
        )
        pid = wait_for(
            lambda: next(
                (
                    row["pid"]
                    for path in Path("/proc").iterdir()
                    if path.name.isdigit()
                    and (row := performance.process(path.name))
                    and row["ppid"] == process.pid
                    and row["name"] in ("qs", "quickshell")
                ),
                None,
            ),
            "shell PID",
        )
        wait_for(lambda: state("nacre", "state"), "shell IPC")
        time.sleep(10)
        ipc("nacre", "close")
        report = {
            "revision": args.revision
            or subprocess.check_output(
                ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
            ).strip(),
            "backend": args.backend,
            "allocator_mmap_threshold": shell_env.get(
                "MALLOC_MMAP_THRESHOLD_", "glibc adaptive default"
            ),
            "shell_sha256": hashlib.sha256(
                b"".join(
                    p.relative_to(shell).as_posix().encode() + p.read_bytes()
                    for p in sorted(shell.rglob("*"))
                    if p.is_file()
                )
            ).hexdigest(),
            "boundary": "Qt key-handler receipt to frameSwapped after geometry, data queue and image loading settle; excludes compositor presentation",
            "scenarios": {},
            "latency": {},
        }

        def sample(name):
            video_before = (
                state(
                    "wallpaperMotion-" + state("performanceFrame", "state")["screen"],
                    "state",
                )
                if name in ("video", "video_resumed")
                else None
            )
            sampled_at = time.monotonic()
            data = performance.idle(
                pid,
                args.idle_seconds
                if name == "static_after_latency"
                else args.sample_seconds,
                0.25,
                detached=False,
            )
            if video_before:
                video_after = state(
                    "wallpaperMotion-" + state("performanceFrame", "state")["screen"],
                    "state",
                )
                data["delivered_video_fps"] = (
                    video_after["measuredFrames"] - video_before["measuredFrames"]
                ) / (time.monotonic() - sampled_at)
            data["mapping_pss_mib"] = mapping(pid)
            meta = state("performanceFrame", "state")
            data["output"] = {
                key: meta[key] for key in ("screen", "width", "height", "favorites")
            }
            report["scenarios"][name] = data
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(report, indent=2) + "\n")
            print(name, "PSS", round(data["pss_mib"]["shell"]["median"], 1), flush=True)

        def measure_actions(label, count):
            report["latency"][label] = {}
            for action, key in (
                [] if args.no_latency else (("launcher", "F8"), ("dashboard", "F9"))
            ):
                timings = []
                for _ in range(count):
                    ipc("nacre", "close")
                    time.sleep(0.4)
                    ipc("performanceFrame", "arm")
                    time.sleep(0.2)
                    if args.backend == "hyprland":
                        subprocess.run(
                            [
                                "hyprctl",
                                "eval",
                                'hl.dispatch(hl.dsp.focus({window="title:^(Nacre Performance Input)$"}))',
                            ],
                            env=env,
                            check=True,
                            capture_output=True,
                        )
                    previous = state("performanceFrame", "state").get("sequence", 0)
                    subprocess.run(
                        ["wtype", "-s", "100", "-k", key, "-s", "300"],
                        env=env,
                        check=True,
                        capture_output=True,
                    )
                    result = wait_for(
                        lambda previous=previous: (
                            value
                            if (value := state("performanceFrame", "state"))
                            and "frameMs" in value
                            and value.get("sequence", 0) > previous
                            else None
                        ),
                        action + " rendered frame",
                    )
                    if action == "launcher" and result.get("readyImages", 0) < 12:
                        raise RuntimeError("Launcher fixture icons are not ready")
                    timings.append(result)
                report["latency"][label][action] = {
                    "samples": timings,
                    "frame_ms": performance.summarize(
                        [value["frameMs"] for value in timings]
                    ),
                    "synchronous_ms": performance.summarize(
                        [value["synchronousMs"] for value in timings]
                    ),
                }
                ipc("nacre", "close")

        sample("fresh_static")
        if not args.no_latency:
            measure_actions("cold", 1)
            time.sleep(6)
        for name, action, close in (
            []
            if args.latency_only
            else [
                ("launcher", ["nacre", "launcher", ""], ["nacre", "close"]),
                *[
                    (f"dashboard_{i}", ["nacre", "tab", str(i)], ["nacre", "close"])
                    for i in range(5)
                ],
                (
                    "settings",
                    ["settingsApp", "open", "appearance"],
                    ["settingsApp", "close"],
                ),
                ("colors", ["colorsApp", "open", ""], ["colorsApp", "close"]),
                ("welcome", ["welcomeApp", "open", ""], ["welcomeApp", "close"]),
                (
                    "wallpaper_picker",
                    ["nacre", "mode", "wallpaper"],
                    ["nacre", "close"],
                ),
                ("ai", ["nacre", "previewLeft"], ["nacre", "close"]),
            ]
        ):
            ipc(*action)
            if name.startswith("dashboard_"):
                ipc("performanceFrame", "pinDashboard")
                wait_for(
                    lambda: (
                        value
                        if (value := state("dashboardState", "state"))
                        and not value.get("busy")
                        and not value.get("queued")
                        and not value.get("message")
                        else None
                    ),
                    "dashboard data",
                )
            time.sleep(2)
            ipc(*close)
            time.sleep(6)
            if not args.quick:
                sample(name + "_closed")
        sample("all_panels_closed")
        meta = state("performanceFrame", "state")
        if meta["favorites"] != 12 or meta["visibleFavorites"] != 12:
            raise RuntimeError("Demo favorites are missing")
        measure_actions("reopen", args.repeats)
        time.sleep(6)
        sample("static_after_latency")
        ipc("performanceFrame", "wallpaper", json.dumps(fixture(home, movie, True)))
        screen_name = state("performanceFrame", "state")["screen"]
        wait_for(
            lambda: (
                value
                if (value := state("wallpaperMotion-" + screen_name, "state"))
                and value.get("firstFrame")
                and value.get("playing")
                and value.get("audioDisabled")
                and not value.get("error")
                else None
            ),
            "playing muted video first frame",
        )
        time.sleep(4)
        sample("video")
        ipc("performanceFrame", "pause", "true")
        time.sleep(6)
        sample("video_paused")
        ipc("performanceFrame", "suspend", "true")
        time.sleep(6)
        sample("video_suspended")
        ipc("performanceFrame", "suspend", "false")
        ipc("performanceFrame", "pause", "false")
        time.sleep(2)
        sample("video_resumed")
        ipc(
            "performanceFrame",
            "wallpaper",
            json.dumps(fixture(home, home / "Pictures/Wallpapers/static.png")),
        )
        time.sleep(6)
        sample("static_after_video")
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n")
        return report
    finally:
        for process in reversed(processes):
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
        shutil.rmtree(runtime)


def check_report(report, budgets):
    """Missing coverage or latency is a failure, not a zero-cost run."""
    failures = []
    for name, ceiling in budgets["ci_pss_mib"].items():
        sample = report.get("scenarios", {}).get(name, {})
        memory = sample.get("pss_mib", {}).get("shell")
        coverage = sample.get("memory_valid_samples", {}).get("pss", {}).get("shell", 0)
        count = len(sample.get("samples", []))
        if not memory or not count or coverage != count:
            failures.append(name + ": incomplete shell PSS coverage")
        elif memory["max"] > ceiling:
            failures.append(
                f"{name}: shell PSS {memory['max']:.1f} MiB exceeds {ceiling} MiB"
            )
    for label, ceiling in budgets["ci_frame_p95_ms"].items():
        for action in ("launcher", "dashboard"):
            capture = report.get("latency", {}).get(label, {}).get(action, {})
            frames = capture.get("frame_ms")
            synchronous = capture.get("synchronous_ms")
            if not frames or not synchronous or not capture.get("samples"):
                failures.append(label + " " + action + ": missing frame timing")
            else:
                if frames["p95"] > ceiling:
                    failures.append(
                        f"{label} {action}: frame p95 {frames['p95']} ms exceeds {ceiling} ms"
                    )
                limit = budgets["ci_synchronous_p95_ms"][label]
                if synchronous["p95"] > limit:
                    failures.append(
                        f"{label} {action}: synchronous p95 {synchronous['p95']} ms exceeds {limit} ms"
                    )
    for name in ("video", "video_resumed"):
        fps = report.get("scenarios", {}).get(name, {}).get("delivered_video_fps")
        if fps is None or fps < budgets["ci_min_delivered_video_fps"]:
            failures.append(name + ": missing or stalled video frame delivery")
    idle = (
        report.get("scenarios", {})
        .get("static_after_latency", {})
        .get("cpu_percent_one_core", {})
        .get("shell")
    )
    if not idle:
        failures.append("missing static idle CPU capture")
    elif idle["mean"] > budgets["ci_idle_cpu_mean_one_core_percent"]:
        failures.append(f"static idle CPU {idle['mean']:.2f}% exceeds budget")
    return failures


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backend", choices=("hyprland", "niri"), default="hyprland")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--labwc", default="labwc")
    parser.add_argument("--labwc-libraries", type=Path)
    parser.add_argument("--wlr-randr", default="wlr-randr")
    parser.add_argument("--sample-seconds", type=float, default=5)
    parser.add_argument("--idle-seconds", type=float, default=30)
    parser.add_argument("--repeats", type=int, default=10)
    parser.add_argument("--quick", action="store_true")
    parser.add_argument("--latency-only", action="store_true")
    parser.add_argument("--no-latency", action="store_true")
    parser.add_argument(
        "--revision", help="Immutable git revision for the before capture"
    )
    parser.add_argument("--keep-fixtures", type=Path)
    parser.add_argument("--check-budgets", action="store_true")
    parser.add_argument(
        "--allocator-mmap-threshold",
        type=int,
        help="Experimental glibc large-buffer threshold in bytes",
    )
    parser.add_argument(
        "--orient-binary", type=Path, help="Pinned native engine for fixture helpers"
    )
    args = parser.parse_args()
    if args.keep_fixtures:
        args.keep_fixtures.mkdir()
        report = run(args, args.keep_fixtures.resolve())
    else:
        with tempfile.TemporaryDirectory(prefix="np-") as directory:
            report = run(args, Path(directory))
    if args.check_budgets:
        failures = check_report(
            report, json.loads((ROOT / "tools/performance-budgets.json").read_text())
        )
        if failures:
            raise SystemExit("\n".join(failures))
        print("Headless memory, CPU and frame budgets passed.")


if __name__ == "__main__":
    main()
