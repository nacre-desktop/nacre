#!/usr/bin/env python3
"""Measure the production shell in a disposable headless/nested Wayland session."""

import argparse
import collections
import json
import math
import os
from pathlib import Path
import re
import shutil
import signal
import shlex
import socket
import subprocess
import sys
import tempfile
import tarfile
import io
import hashlib
import importlib.util
import time
import threading

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
    property string perfWallpaper: ""
    property string perfPrimary: ""
    function perfChooseWallpaper(path, primary) {
        perfSequence++;
        perfAction = "wallpaper-colors";
        perfWallpaper = path;
        perfPrimary = primary.toLowerCase().replace("#", "");
        perfResult = {sequence: perfSequence};
        perfStarted = Date.now();
        NacreWallpapers.setWallpaper(path);
    }
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
            if (root.perfAction === "wallpaper-colors") {
                const active = NacrePresentation.active;
                if (active?.poster !== root.perfWallpaper || active?.colours?.primary?.toLowerCase().replace("#", "") !== root.perfPrimary) return;
                if (root.perfImages(root.contentItem).pending) return;
                root.perfResult = Object.assign({}, root.perfResult, {shellFrameMs: Date.now() - root.perfStarted, chosenAtMs: root.perfStarted, shellFrameAtMs: Date.now(), primary: root.perfPrimary, poster: root.perfWallpaper});
                root.perfStarted = 0;
                return;
            }
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
        function chooseWallpaper(path: string, primary: string): void { root.perfChooseWallpaper(path, primary); }
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


def paging():
    """Host swap traffic disqualifies small memory/latency comparisons."""
    fields = dict(
        line.split() for line in Path("/proc/vmstat").read_text().splitlines()
    )
    return {key: int(fields[key]) for key in ("pswpin", "pswpout")}


FULL_IDLE_PHASES = ("fresh_static", "all_panels_closed", "static_after_latency")


def idle_sample_duration(name, sample_seconds, idle_seconds, full_idle_windows=False):
    """Choose an observation duration, preserving legacy defaults unless opted in."""
    if name == "static_after_latency":
        requested = idle_seconds
    else:
        requested = sample_seconds
    if full_idle_windows and name in FULL_IDLE_PHASES:
        return max(60.0, requested)
    return requested


def observe_idle_phase(
    name, duration, idle, paging_reader, clock, page_size_bytes, full_idle_windows=False
):
    """Attach isolated host-counter evidence to a named idle observation."""
    if not full_idle_windows or name not in FULL_IDLE_PHASES:
        return idle(duration)

    start = paging_reader()
    started_at = clock()
    data = idle(duration)
    idle_finished_at = clock()
    end = paging_reader()
    counter_read_finished_at = clock()
    delta_in_pages = end["pswpin"] - start["pswpin"]
    delta_out_pages = end["pswpout"] - start["pswpout"]
    data["paging_window"] = {
        "start": start,
        "end": end,
        "page_size_bytes": page_size_bytes,
        "elapsed_s": idle_finished_at - started_at,
        "counter_bracket_elapsed_s": counter_read_finished_at - started_at,
        "delta_in_pages": delta_in_pages,
        "delta_in_bytes": delta_in_pages * page_size_bytes,
        "delta_out_pages": delta_out_pages,
        "delta_out_bytes": delta_out_pages * page_size_bytes,
    }
    return data


def idle_phase_failures(report):
    """Reject incomplete named windows without changing the whole-arm limit."""
    if "wallpaper_frame_smoke" in report:
        return []
    failures = []
    scenarios = report.get("scenarios", {})
    for name in FULL_IDLE_PHASES:
        window = scenarios.get(name, {}).get("paging_window")
        if not isinstance(window, dict):
            failures.append(name + ": missing full idle paging window")
            continue
        start, end = window.get("start"), window.get("end")
        page_size = window.get("page_size_bytes")
        elapsed = window.get("elapsed_s")
        if (
            not isinstance(start, dict)
            or not isinstance(end, dict)
            or any(
                not isinstance(counter, int) or isinstance(counter, bool)
                for counters in (start, end)
                for counter in (counters.get("pswpin"), counters.get("pswpout"))
            )
            or any(
                counter < 0
                for counters in (start, end)
                for counter in (counters.get("pswpin"), counters.get("pswpout"))
                if isinstance(counter, int) and not isinstance(counter, bool)
            )
            or not isinstance(page_size, int)
            or isinstance(page_size, bool)
            or page_size <= 0
            or not isinstance(elapsed, (int, float))
            or isinstance(elapsed, bool)
            or not math.isfinite(elapsed)
        ):
            failures.append(name + ": incomplete idle paging counters or timing")
            continue
        delta_in_pages = end["pswpin"] - start["pswpin"]
        delta_out_pages = end["pswpout"] - start["pswpout"]
        if delta_in_pages < 0 or delta_out_pages < 0:
            failures.append(name + ": idle paging counter reset")
            continue
        if (
            window.get("delta_in_pages") != delta_in_pages
            or window.get("delta_in_bytes") != delta_in_pages * page_size
            or window.get("delta_out_pages") != delta_out_pages
            or window.get("delta_out_bytes") != delta_out_pages * page_size
        ):
            failures.append(name + ": inconsistent idle paging deltas")
        if elapsed < 60:
            failures.append(name + ": idle paging window shorter than 60 seconds")
    return failures


def machine_metadata(proc=Path("/proc"), sysroot=Path("/sys"), etc=Path("/etc")):
    """Read the requested machine facts without hostnames or configuration changes."""
    errors = []

    def read(path):
        try:
            return path.read_text()
        except OSError as error:
            errors.append(path.name + ": " + type(error).__name__)
            return ""

    cpus = [
        line.split(":", 1)[1].strip()
        for line in read(proc / "cpuinfo").splitlines()
        if line.startswith("model name") and ":" in line
    ]
    memory = dict(
        line.split(":", 1)
        for line in read(proc / "meminfo").splitlines()
        if ":" in line
    )
    total = memory.get("MemTotal", "").split()
    distro = dict(
        line.split("=", 1)
        for line in read(etc / "os-release").splitlines()
        if "=" in line and not line.startswith("#")
    )
    governors = sorted(
        {
            read(path).strip()
            for path in (sysroot / "devices/system/cpu/cpufreq").glob(
                "policy*/scaling_governor"
            )
        }
    )
    return {
        "cpu": cpus[0] if cpus else "unavailable",
        "logical_cpus": len(cpus),
        "ram_bytes": int(total[0]) * 1024 if total and total[0].isdigit() else None,
        "kernel": read(proc / "sys/kernel/osrelease").strip() or "unavailable",
        "distro": distro.get("PRETTY_NAME", distro.get("NAME", "unavailable")).strip(
            '"'
        ),
        "cpu_governors": governors,
        "errors": errors,
    }


def desktop_palette_result(chosen_at_ns, expected_colors, frames):
    """Aggregate only attributed compositor pixels, never file/state acknowledgements."""
    required = ("shell", "gtk", "qt", "kitty", "lock")
    if set(expected_colors) != set(required):
        raise ValueError("Expected colors must name every desktop consumer")
    missing = {}
    accepted = {}
    for name in required:
        frame = frames.get(name)
        if not frame or frame.get("boundary") != "compositor-pixel-capture":
            missing[name] = "no attributed compositor frame"
            continue
        color = str(frame.get("color", "")).lower().removeprefix("#")
        expected = str(expected_colors[name]).lower().removeprefix("#")
        region = frame.get("region")
        digest = frame.get("capture_sha256", "")
        observed = frame.get("observed_at_ns")
        if color != expected:
            missing[name] = "rendered color has not converged"
        elif (
            not isinstance(region, list)
            or len(region) != 4
            or region[2] <= 0
            or region[3] <= 0
            or not re.fullmatch("[0-9a-f]{64}", digest)
        ):
            missing[name] = "missing pixel region or capture digest"
        elif not isinstance(observed, int) or observed < chosen_at_ns:
            missing[name] = "frame predates the selection or its clock is invalid"
        else:
            accepted[name] = frame
    complete = not missing
    return {
        "complete": complete,
        "missing": missing,
        "frames": accepted,
        "visible_upper_bound_ms": (
            max(frame["observed_at_ns"] for frame in accepted.values()) - chosen_at_ns
        )
        / 1e6
        if complete
        else None,
        "boundary": "selection to last attributed compositor pixel capture, including capture overhead",
    }


def quiet_window_failures(report):
    """Owner's comparison gate; independent of the stricter CI budgets."""
    failures = []
    start, end = report.get("paging_start"), report.get("paging_end")
    page_size = report.get("page_size_bytes")
    counter_values = (
        [start.get(key) for key in ("pswpin", "pswpout")]
        + [end.get(key) for key in ("pswpin", "pswpout")]
        if isinstance(start, dict) and isinstance(end, dict)
        else []
    )
    if (
        not start
        or not end
        or not isinstance(page_size, int)
        or isinstance(page_size, bool)
        or page_size <= 0
        or any(
            not isinstance(counter, int)
            or isinstance(counter, bool)
            or counter < 0
            for counter in counter_values
        )
    ):
        failures.append("missing host paging counters or page size")
    else:
        if report.get("full_idle_windows") is True:
            failures.extend(idle_phase_failures(report))
        pages = end["pswpin"] - start["pswpin"]
        if pages < 0:
            failures.append("host paging counter reset during capture")
        elif pages * page_size >= 1048576:
            failures.append("host swap-in reached the 1 MiB limit")
    for name, scenario in report.get("scenarios", {}).items():
        count = len(scenario.get("samples", []))
        coverage = (
            scenario.get("memory_valid_samples", {}).get("swap_pss", {}).get("shell", 0)
        )
        swap = scenario.get("swap_pss_mib", {}).get("shell")
        if not count or coverage != count or not swap:
            failures.append(name + ": incomplete measured-process SwapPss coverage")
        elif swap["max"] != 0:
            failures.append(name + ": measured processes have swapped pages")
        for sample in scenario.get("samples", []):
            processes = sample.get("processes")
            if not processes or any(
                row.get("swap_pss_mib") is None for row in processes
            ):
                failures.append(name + ": incomplete per-process SwapPss coverage")
                break
            if any(row["swap_pss_mib"] != 0 for row in processes):
                failures.append(name + ": measured processes have swapped pages")
                break
    if not report.get("scenarios"):
        failures.append("missing measured-process scenarios")
    if "wallpaper_frame_smoke" in report:
        samples = report.get("palette_process_swap_samples", [])
        if not samples or any(not sample.get("processes") for sample in samples):
            failures.append("missing palette-process SwapPss coverage during selection")
        for sample in samples:
            if any(
                row.get("swap_pss_bytes") != 0 for row in sample.get("processes", [])
            ):
                failures.append("palette processes swapped or have unknown SwapPss")
                break
    return failures


def monitor_palette_swap(stop, samples):
    """Cover witnesses/compositors as well as shell descendants during the action."""
    while True:
        processes = []
        for row in performance.snapshot(os.getpid(), detached=False).values():
            try:
                text = Path(f"/proc/{row['pid']}/smaps_rollup").read_text()
                value = re.search(r"(?m)^SwapPss:\s+(\d+) kB$", text)
                swap = int(value[1]) * 1024 if value else None
            except (FileNotFoundError, ProcessLookupError):
                continue
            except OSError:
                # Unreadable live processes are unknown; never assume zero.
                swap = None
            current = performance.process(row["pid"])
            if current is None or current["start"] != row["start"]:
                continue
            processes.append(
                {"pid": row["pid"], "name": row["name"], "swap_pss_bytes": swap}
            )
        samples.append({"at_ns": time.time_ns(), "processes": processes})
        if stop.wait(0.25):
            return


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


def disable_speed_pass(shell):
    """Same-source control: keep concurrent UI/helper changes, disable this pass."""
    changes = {
        "modules/background/NacreWallpaperScene.qml": (
            "readonly property bool motionHidden: WallpaperPlayback.sleeping || WallpaperPlayback.locked || (!pickerOpen && NacreCompositor.windows.some(client => client.workspace?.id === workspaceId && client.fullscreen))",
            "readonly property bool motionHidden: false",
        ),
        "modules/dashboard/NacreDashboardPanel.qml": (
            "            asynchronous: true",
            "            asynchronous: false",
        ),
        "services/NacreMenuContext.qml": (
            "Qt.callLater(root.refreshOnOpen);",
            "root.refreshOnOpen();",
        ),
        "services/NacreDashboardState.qml": (
            "Qt.callLater(root.refresh)",
            "root.refresh()",
        ),
    }
    for relative, (before, after) in changes.items():
        path = shell / relative
        source = path.read_text()
        if source.count(before) != 1:
            raise RuntimeError(
                "Control no longer matches the production source: " + relative
            )
        path.write_text(source.replace(before, after))


def prepare(directory, revision=None, control=False, helper_binary=None):
    from PIL import Image

    home = directory / "home"
    shell = directory / "shell"
    source = ROOT
    if revision:
        archive = subprocess.check_output(
            [
                "git",
                "archive",
                revision,
                "nacre/shell",
                "nacre/shell-tools",
                "nacre/shell-cli",
            ],
            cwd=ROOT,
        )
        source = directory / "source"
        source.mkdir()
        with tarfile.open(fileobj=io.BytesIO(archive)) as files:
            files.extractall(source, filter="data")
    if (source / "nacre/shell-cli").is_dir():
        adapter = home / ".local/share/nacre/shell-cli"
        adapter.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(
            source / "nacre/shell-cli",
            adapter,
            ignore=shutil.ignore_patterns("__pycache__", "orient"),
        )
    shutil.copytree(source / "nacre/shell", shell)
    if control:
        disable_speed_pass(shell)
    tools = home / ".local/share/nacre/shell/tools"
    shutil.copytree(
        source / "nacre/shell-tools",
        tools,
        ignore=shutil.ignore_patterns("tests", "__pycache__"),
    )
    native_commands = any(
        "NacrePaths.helperBinary" in path.read_text()
        for path in (shell / "services").glob("*.qml")
    )
    if native_commands:
        if helper_binary is None or not Path(helper_binary).is_file():
            raise ValueError(
                "This revision needs --helper-binary built from the exact revision under test"
            )
        shutil.copy2(Path(helper_binary).resolve(), tools / "nacre-helper")
        bridge = home / ".local/share/nacre/shell/bin/nacre_shell"
        bridge.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(tools / "cli-bridge.sh", bridge)
        bridge.chmod(0o700)
    # Both adapter generations resolve shared state from their source sibling.
    (home / ".local/share/nacre/shell-tools").symlink_to(
        tools, target_is_directory=True
    )
    # Native helpers resolve sibling source assets through the installed layout.
    (tools.parent / "source").symlink_to(shell, target_is_directory=True)
    (tools.parent / "shell-cli").symlink_to(
        home / ".local/share/nacre/shell-cli", target_is_directory=True
    )
    runtime_python = home / ".local/share/nacre/palette-runtime/venv"
    runtime_bin = runtime_python / "bin"
    runtime_bin.mkdir(parents=True)
    (runtime_bin / "python").symlink_to(sys.executable)
    adapter_cli = runtime_bin / "nacre_shell"
    adapter_cli.write_text(
        "#!/usr/bin/env sh\nexec "
        + shlex.quote(sys.executable)
        + ' -m nacre_shell "$@"\n'
    )
    adapter_cli.chmod(0o700)
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
    if not (tools / "shell-supervisor.py").is_file():
        result = subprocess.run(
            [str(tools / "nacre-helper"), "shell-environment"],
            env=environment,
            capture_output=True,
            text=True,
            check=True,
            timeout=25,
        )
        value = json.loads(result.stdout)
        if not isinstance(value, dict) or any(
            not isinstance(v, str) for v in value.values()
        ):
            raise ValueError("Invalid native shell environment")
        return {**environment, **value}
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
    home, shell, movie = prepare(
        directory, args.revision, args.control, args.helper_binary
    )
    # Unix-domain socket paths (including Hyprland signatures) must fit 108 bytes.
    runtime = Path(tempfile.mkdtemp(prefix="npr-", dir="/tmp"))
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
        PYTHONPATH=str(home / ".local/share/nacre/shell-cli/src"),
        DBUS_SESSION_BUS_ADDRESS="unix:path=" + str(runtime / "absent-session-bus"),
        DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + str(runtime / "absent-system-bus"),
        HYPRLAND_NO_SYSTEMD="1",
        GVFS_DISABLE_FUSE="1",
    )
    if args.orient_binary:
        env["NACRE_ORIENT_BINARY"] = str(args.orient_binary.resolve(strict=True))
    processes = []
    palette_monitor = None
    palette_stop = threading.Event()
    palette_swap_samples = []

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
            WLR_RENDERER_ALLOW_SOFTWARE="1",
            WLR_LIBINPUT_NO_DEVICES="1",
            WLR_HEADLESS_OUTPUTS="1",
        )
        parent_env.pop("WLR_RENDERER", None)
        if args.parent_renderer != "auto":
            parent_env["WLR_RENDERER"] = args.parent_renderer
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
            niri_socket = wait_for(
                lambda: next(iter(runtime.glob("niri.*.sock")), None), "niri socket"
            )
            env["NIRI_SOCKET"] = str(niri_socket)
        if args.backend != "labwc":
            wait_for(lambda: (runtime / "wayland-1").exists(), "nested Wayland socket")
            env["WAYLAND_DISPLAY"] = "wayland-1"
        shell_env = shell_environment(home / ".local/share/nacre/shell/tools", env)
        if args.adaptive_allocator or args.control:
            shell_env.pop("MALLOC_MMAP_THRESHOLD_", None)
        if args.allocator_mmap_threshold:
            shell_env["MALLOC_MMAP_THRESHOLD_"] = str(args.allocator_mmap_threshold)
        launch = ["quickshell", "-p", str(shell), "-n"]
        if args.supervised_shell:
            private_config = home / ".config/quickshell/nacre"
            private_config.parent.mkdir(parents=True, exist_ok=True)
            private_config.symlink_to(shell, target_is_directory=True)
            qs = home / ".local/share/nacre/shell/bin/qs"
            qs.parent.mkdir(parents=True, exist_ok=True)
            qs.write_text(
                "#!/bin/sh\nexec " + shlex.quote(shutil.which("quickshell")) + ' "$@"\n'
            )
            qs.chmod(0o700)
            supervisor = home / ".local/share/nacre/shell/tools/shell-supervisor.py"
            launch = (
                [sys.executable, str(supervisor)]
                if supervisor.is_file()
                else [str(supervisor.with_name("nacre-helper")), "shell-supervisor"]
            )
        process = start(["dbus-run-session", "--", *launch], "shell", shell_env)
        pid = wait_for(
            lambda: next(
                (
                    row["pid"]
                    for row in performance.snapshot(
                        process.pid, detached=False
                    ).values()
                    if row["name"] in ("qs", "quickshell")
                ),
                None,
            ),
            "shell PID",
        )
        # Witnesses and CLI calls must share the shell's disposable bus. Never
        # inherit the owner's session bus or create a second notification domain.
        private_environment = dict(
            field.split(b"=", 1)
            for field in Path(f"/proc/{pid}/environ").read_bytes().split(b"\0")
            if b"=" in field
        )
        shell_env["DBUS_SESSION_BUS_ADDRESS"] = private_environment[
            b"DBUS_SESSION_BUS_ADDRESS"
        ].decode()
        wait_for(lambda: state("nacre", "state"), "shell IPC")
        time.sleep(10)
        ipc("nacre", "close")
        versions = {}
        for name, command in {
            "quickshell": ["quickshell", "--version"],
            "niri": ["niri", "--version"],
            "hyprland": ["Hyprland", "--version"],
            "compositor": [
                "Hyprland" if args.backend == "hyprland" else "niri",
                "--version",
            ],
            "qt": ["pkg-config", "--modversion", "Qt6Core"],
            "gtk3": ["pkg-config", "--modversion", "gtk+-3.0"],
            "kitty": ["kitty", "--version"],
            "orient": [
                str(args.orient_binary or Path(sys.prefix) / "bin/orient"),
                "--version",
            ],
        }.items():
            try:
                result = subprocess.run(
                    command,
                    env=shell_env,
                    capture_output=True,
                    text=True,
                    timeout=5,
                    check=True,
                )
                versions[name] = result.stdout.strip().splitlines()[0]
            except (OSError, subprocess.SubprocessError, IndexError):
                versions[name] = "unavailable"
        report = {
            "machine": machine_metadata(),
            "versions": versions,
            "control": args.control,
            "budget_tier": args.budget_tier,
            "parent_renderer": args.parent_renderer,
            "harness_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
            "sampler_sha256": hashlib.sha256(
                Path(performance.__file__).read_bytes()
            ).hexdigest(),
            "helper_sha256": hashlib.sha256(
                (home / ".local/share/nacre/shell/tools/nacre-helper").read_bytes()
            ).hexdigest()
            if args.helper_binary
            else None,
            "palette_probe_sha256": {
                name: hashlib.sha256(
                    (args.palette_probes / name).read_bytes()
                ).hexdigest()
                for name in ("gtk-palette", "qt-palette")
            }
            if args.palette_probes
            else {},
            "paging_start": paging(),
            "paging_end": paging(),
            "page_size_bytes": os.sysconf("SC_PAGE_SIZE"),
            "full_idle_windows": args.full_idle_windows,
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
            data = observe_idle_phase(
                name,
                idle_sample_duration(
                    name,
                    args.sample_seconds,
                    args.idle_seconds,
                    full_idle_windows=args.full_idle_windows,
                ),
                lambda duration: performance.idle(
                    pid, duration, 0.25, detached=False
                ),
                paging,
                time.monotonic,
                report["page_size_bytes"],
                full_idle_windows=args.full_idle_windows,
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
            report["paging_end"] = paging()
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
                    try:
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
                    except RuntimeError as failure:
                        diagnostic = json.dumps(state("performanceFrame", "state"))[
                            :1500
                        ]
                        raise RuntimeError(
                            f"{failure}; panel state: {diagnostic}"
                        ) from failure
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

        if args.wallpaper_frame_smoke:
            from PIL import Image

            config = home / ".config/nacre"
            (config / "cli.json").write_text(
                json.dumps({"version": 1, "wallpaper": {"postHook": None}})
            )
            (config / "wallpaper-picker.json").write_text(
                json.dumps(
                    {
                        "version": 1,
                        "paletteMode": "dark",
                        "palettePersonality": "natural",
                    }
                )
            )
            scheme = json.loads(
                (
                    home / ".local/share/nacre/shell/tools/reference-style.json"
                ).read_text()
            )
            scheme["name"] = "dynamic"
            (home / ".local/state/nacre/scheme.json").write_text(json.dumps(scheme))
            selected = home / "Pictures/Wallpapers/color-selection.png"
            Image.new("RGB", (320, 200), "#1752a4").save(selected)
            palette_cli = home / ".local/share/nacre/shell/bin/nacre_shell"
            prepared = subprocess.run(
                [
                    str(palette_cli),
                    "wallpaper",
                    "--print",
                    str(selected),
                    "--mode",
                    "dark",
                ],
                env=shell_env,
                text=True,
                capture_output=True,
                check=False,
                timeout=30,
            )
            if prepared.returncode:
                raise RuntimeError(
                    "Palette precomputation failed: "
                    + prepared.stderr[-1000:]
                    + prepared.stdout[-500:]
                )
            palette = json.loads(prepared.stdout)
            primary = palette["colours"]["primary"]
            witnesses = {}
            if args.palette_probes:
                qt_settings = home / ".config/qt6ct/qt6ct.conf"
                qt_settings.parent.mkdir(parents=True, exist_ok=True)
                qt_settings.write_text(
                    "[Appearance]\ncustom_palette=true\ncolor_scheme_path="
                    + str(config / "qt.conf")
                    + "\n"
                )
                initial = home / "Pictures/Wallpapers/initial-colors.png"
                Image.new("RGB", (320, 200), "#8a3510").save(initial)
                published = subprocess.run(
                    [
                        str(palette_cli),
                        "wallpaper",
                        "--file",
                        str(initial),
                        "--mode",
                        "dark",
                    ],
                    env=shell_env,
                    check=False,
                    capture_output=True,
                    text=True,
                    timeout=60,
                )
                if published.returncode:
                    raise RuntimeError(
                        "Initial palette publication failed: "
                        + published.stderr[-1500:]
                        + published.stdout[-500:]
                    )
                initial_palette = json.loads(
                    (home / ".local/state/nacre/scheme.json").read_text()
                )
                for consumer in ("gtk", "qt"):
                    binary = args.palette_probes / (consumer + "-palette")
                    if not binary.is_file():
                        raise RuntimeError(
                            "Missing prebuilt palette witness: " + str(binary)
                        )
                    witness_env = dict(
                        shell_env, GDK_BACKEND="wayland", QT_QPA_PLATFORMTHEME="qt6ct"
                    )
                    witnesses[consumer] = start(
                        [str(binary.resolve())], consumer + "-witness", witness_env
                    )
                    wait_for(
                        lambda consumer=consumer: (
                            '"primary"'
                            in (directory / (consumer + "-witness.log")).read_text()
                        ),
                        consumer + " initial paint",
                    )
                kitty_config = home / ".config/kitty/kitty.conf"
                kitty_config.parent.mkdir(parents=True, exist_ok=True)
                kitty_config.write_text(
                    "include " + str(config / "kitty-colors.conf") + "\n"
                )
                kitty_socket = runtime / "kitty-witness.sock"
                start(
                    [
                        "kitty",
                        "--config",
                        str(kitty_config),
                        "--title",
                        "Nacre palette Kitty witness",
                        "--",
                        sys.executable,
                        str(ROOT / "tools/performance-probes/kitty-palette.py"),
                        "--socket",
                        str(kitty_socket),
                    ],
                    "kitty-witness",
                    shell_env,
                )
                wait_for(kitty_socket.exists, "Kitty witness socket")
                kitty_peer = socket.socket(socket.AF_UNIX)
                kitty_peer.settimeout(3)
                kitty_peer.connect(str(kitty_socket))
                kitty_stream = kitty_peer.makefile("rwb", buffering=0)
                json.loads(kitty_stream.readline())
                initial_kitty_blue = re.search(
                    r"(?m)^color12 #([0-9a-fA-F]{6})$",
                    (config / "kitty-colors.conf").read_text(),
                )[1]
                if args.backend == "niri":
                    # Arrange only private witnesses before starting the clock.
                    windows = json.loads(
                        subprocess.check_output(
                            ["niri", "msg", "-j", "windows"],
                            env=env,
                            text=True,
                            timeout=5,
                        )
                    )
                    for index, title in enumerate(
                        (
                            "Nacre palette GTK witness",
                            "Nacre palette Qt witness",
                            "Nacre palette Kitty witness",
                        )
                    ):
                        window = next(v for v in windows if v["title"] == title)
                        ident = str(window["id"])
                        for action in (
                            ["move-window-to-floating"],
                            ["set-window-width", "320"],
                            ["set-window-height", "240"],
                            [
                                "move-floating-window",
                                "--x",
                                str(40 + index * 400),
                                "--y",
                                "100",
                            ],
                        ):
                            subprocess.run(
                                ["niri", "msg", "action", *action, "--id", ident],
                                env=env,
                                check=True,
                                capture_output=True,
                                timeout=5,
                            )
            palette_monitor = threading.Thread(
                target=monitor_palette_swap, args=(palette_stop, palette_swap_samples)
            )
            palette_monitor.start()
            ipc("performanceFrame", "chooseWallpaper", str(selected), primary)
            result = wait_for(
                lambda: (
                    value
                    if (value := state("performanceFrame", "state"))
                    and "shellFrameAtMs" in value
                    else None
                ),
                "wallpaper shell color frame",
                timeout=30,
            )
            pixels = directory / "wallpaper-frame.png"
            capture_started = time.time_ns()
            subprocess.run(
                ["grim", str(pixels)],
                env=env,
                check=True,
                capture_output=True,
                timeout=10,
            )
            capture_finished = time.time_ns()
            with Image.open(pixels) as screenshot:
                rgb = screenshot.convert("RGB")
                wanted = tuple(
                    int(primary[index : index + 2], 16) for index in (0, 2, 4)
                )
                primary_pixels = sum(
                    pixel == wanted for pixel in rgb.get_flattened_data()
                )
            if primary_pixels == 0:
                raise RuntimeError(
                    "Expected palette primary is absent from compositor pixels"
                )
            report["wallpaper_frame_smoke"] = {
                "palette_cache_state": "selected image precomputed; warm-cache functional smoke",
                "shell": result,
                "screenshot": {
                    "sha256": hashlib.sha256(pixels.read_bytes()).hexdigest(),
                    "exact_primary_pixels": primary_pixels,
                    "capture_started_ns": capture_started,
                    "capture_finished_ns": capture_finished,
                    "boundary": "compositor capture; any scene occurrence, not per-client attribution",
                },
                "whole_desktop_complete": False,
                "expected": {
                    "gtk": primary,
                    "qt": primary,
                    "kitty_background": palette["colours"]["surfaceContainerLow"],
                },
                "missing_consumers": ["gtk", "qt", "kitty", "actual-lock-surface"],
                "boundary": "controller setWallpaper to shell frameSwapped, then whole-scene compositor pixel check",
            }
            if witnesses:
                # A bounded observation window exposes clients that never repaint;
                # paint callbacks remain diagnostic, not compositor frame evidence.
                time.sleep(5)
                report["wallpaper_frame_smoke"]["paint_witnesses"] = {
                    name: {
                        "exit_code": process.poll(),
                        "observations": [
                            json.loads(line)
                            for line in (directory / (name + "-witness.log"))
                            .read_text()
                            .splitlines()
                            if line.startswith('{"consumer":')
                        ],
                        "diagnostics": (
                            directory / (name + "-witness.log")
                        ).read_text()[-2000:],
                        "boundary": "application paint callback; compositor attribution still required",
                    }
                    for name, process in witnesses.items()
                }
                kitty_stream.write(b'{"action":"query"}\n')
                report["wallpaper_frame_smoke"]["kitty_state"] = json.loads(
                    kitty_stream.readline()
                )
                if args.backend in ("hyprland", "niri"):
                    if args.backend == "hyprland":
                        clients = json.loads(
                            subprocess.check_output(
                                ["hyprctl", "clients", "-j"],
                                env=env,
                                text=True,
                                timeout=5,
                            )
                        )
                    else:
                        windows = json.loads(
                            subprocess.check_output(
                                ["niri", "msg", "-j", "windows"],
                                env=env,
                                text=True,
                                timeout=5,
                            )
                        )
                        active = {
                            v["id"]
                            for v in json.loads(
                                subprocess.check_output(
                                    ["niri", "msg", "-j", "workspaces"],
                                    env=env,
                                    text=True,
                                    timeout=5,
                                )
                            )
                            if v["is_active"]
                        }
                        clients = []
                        for window in windows:
                            layout = window.get("layout", {})
                            position = layout.get("tile_pos_in_workspace_view")
                            if position is None or window["workspace_id"] not in active:
                                continue
                            clients.append(
                                {
                                    "title": window["title"],
                                    "at": [
                                        round(a + b)
                                        for a, b in zip(
                                            position,
                                            layout["window_offset_in_tile"],
                                            strict=True,
                                        )
                                    ],
                                    "size": layout["window_size"],
                                }
                            )
                    capture_started = time.time_ns()
                    subprocess.run(
                        ["grim", str(pixels)],
                        env=env,
                        check=True,
                        capture_output=True,
                        timeout=10,
                    )
                    capture_finished = time.time_ns()
                    with Image.open(pixels) as image:
                        rgb = image.convert("RGB")
                        crops = {}
                        for name, title in (
                            ("gtk", "Nacre palette GTK witness"),
                            ("qt", "Nacre palette Qt witness"),
                            ("kitty", "Nacre palette Kitty witness"),
                        ):
                            client = next(
                                (v for v in clients if v.get("title") == title), None
                            )
                            if client is None:
                                crops[name] = {"missing": "witness surface is absent"}
                                continue
                            x, y = client["at"]
                            width, height = client["size"]
                            region = [x, y, width, height]
                            expected_color = (
                                palette["colours"]["surface"]
                                if name == "gtk"
                                else primary
                                if name != "kitty"
                                else re.search(
                                    r"(?m)^color12 #([0-9a-fA-F]{6})$",
                                    (config / "kitty-colors.conf").read_text(),
                                )[1]
                            )
                            wanted = tuple(
                                int(expected_color[i : i + 2], 16) for i in (0, 2, 4)
                            )
                            count = sum(
                                pixel == wanted
                                for pixel in rgb.crop(
                                    (x, y, x + width, y + height)
                                ).get_flattened_data()
                            )
                            observations = (
                                report["wallpaper_frame_smoke"]["paint_witnesses"]
                                .get(name, {})
                                .get("observations", [])
                            )
                            initial_color = (
                                observations[0]["primary"]
                                if observations
                                else initial_kitty_blue
                            )
                            if name == "gtk":
                                initial_color = initial_palette["colours"]["surface"]
                            old = tuple(
                                int(initial_color[i : i + 2], 16) for i in (0, 2, 4)
                            )
                            previous_count = sum(
                                pixel == old
                                for pixel in rgb.crop(
                                    (x, y, x + width, y + height)
                                ).get_flattened_data()
                            )
                            crops[name] = {
                                "region": region,
                                "initial_color": initial_color,
                                "role": "surface"
                                if name == "gtk"
                                else "color12"
                                if name == "kitty"
                                else "primary",
                                "previous_primary_pixels": previous_count,
                                "bounds_inside_capture": 0 <= x
                                and 0 <= y
                                and x + width <= rgb.width
                                and y + height <= rgb.height,
                                "exact_color_pixels": count,
                                "color": expected_color if count else None,
                                "expected_color": expected_color,
                                "capture_started_ns": capture_started,
                                "observed_at_ns": capture_finished,
                                "capture_sha256": hashlib.sha256(
                                    pixels.read_bytes()
                                ).hexdigest(),
                                "boundary": "compositor-pixel-capture",
                            }
                    report["wallpaper_frame_smoke"]["witness_pixels"] = crops
                    for name, crop in crops.items():
                        if crop.get("exact_color_pixels", 0) and crop.get(
                            "bounds_inside_capture"
                        ):
                            report["wallpaper_frame_smoke"]["missing_consumers"].remove(
                                name
                            )
                kitty_stream.write(b'{"action":"quit"}\n')
                kitty_stream.close()
                kitty_peer.close()
                if args.lock_frame_smoke:
                    lock_config = home / ".config/hypr/hyprlock.conf"
                    wait_for(
                        lambda: (
                            lock_config.is_file() and primary in lock_config.read_text()
                        ),
                        "generated lock palette",
                    )
                    lock = start(
                        [
                            "hyprlock",
                            "--config",
                            str(lock_config),
                            "--grace",
                            "0",
                            "--no-fade-in",
                            "--immediate-render",
                        ],
                        "lock-witness",
                        shell_env,
                    )
                    time.sleep(2)
                    lock_pixels = directory / "actual-lock-frame.png"
                    started = time.time_ns()
                    captured = subprocess.run(
                        ["grim", str(lock_pixels)],
                        env=env,
                        capture_output=True,
                        text=True,
                        timeout=10,
                    )
                    finished = time.time_ns()
                    lock_result = {
                        "options": [
                            "--grace",
                            "0",
                            "--no-fade-in",
                            "--immediate-render",
                        ],
                        "session_locked": "onLockLocked called"
                        in (directory / "lock-witness.log").read_text(),
                        "exit_code": lock.poll(),
                        "capture_exit_code": captured.returncode,
                        "diagnostics": (directory / "lock-witness.log").read_text()[
                            -1500:
                        ],
                        "capture_stderr": captured.stderr[-500:],
                    }
                    if captured.returncode == 0:
                        with Image.open(lock_pixels) as image:
                            wanted = tuple(
                                int(primary[i : i + 2], 16) for i in (0, 2, 4)
                            )
                            count = sum(
                                pixel == wanted
                                for pixel in image.convert("RGB").get_flattened_data()
                            )
                        lock_result.update(
                            exact_primary_pixels=count,
                            capture_started_ns=started,
                            observed_at_ns=finished,
                            capture_sha256=hashlib.sha256(
                                lock_pixels.read_bytes()
                            ).hexdigest(),
                            boundary="private nested session lock compositor capture; verify lock acquired in diagnostics",
                        )
                    if not lock_result["session_locked"]:
                        lock_result["boundary"] = (
                            "ordinary scene capture; actual nested session lock was not acquired"
                        )
                        lock_result["missing"] = "actual lock-surface pixel evidence"
                    report["wallpaper_frame_smoke"]["actual_lock"] = lock_result
                    if lock_result["session_locked"] and lock_result.get(
                        "exact_primary_pixels", 0
                    ):
                        report["wallpaper_frame_smoke"]["missing_consumers"].remove(
                            "actual-lock-surface"
                        )
        if palette_monitor:
            palette_stop.set()
            palette_monitor.join()
            report["palette_process_swap_samples"] = palette_swap_samples
        sample("fresh_static")
        if args.wallpaper_frame_smoke:
            report["paging_end"] = paging()
            report["paging_gate_failures"] = quiet_window_failures(report)
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(report, indent=2) + "\n")
            return report
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
        report["paging_end"] = paging()
        report["paging_gate_failures"] = quiet_window_failures(report)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n")
        return report
    except Exception:
        for name in ("parent", "compositor"):
            log = directory / (name + ".log")
            if log.exists():
                print(
                    name
                    + " startup diagnostics:\n"
                    + "\n".join(log.read_text(errors="replace").splitlines()[-15:]),
                    file=sys.stderr,
                )
        raise
    finally:
        if palette_monitor and palette_monitor.is_alive():
            palette_stop.set()
            palette_monitor.join()
        compositor_logs = list(runtime.glob("hypr/*/hyprland.log"))
        if compositor_logs:
            tail = compositor_logs[0].read_text(errors="replace").splitlines()[-50:]
            (directory / "compositor-final.log").write_text("\n".join(tail) + "\n")
            if not args.output.exists():
                print(
                    "Private compositor final diagnostics:\n" + "\n".join(tail[-20:]),
                    file=sys.stderr,
                )
        for process in reversed(processes):
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            if process.poll() is None:
                try:
                    process.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    pass
            # A wrapper may exit before children or D-Bus activated services.
            # Every group was created by start_new_session in this harness.
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
        shutil.rmtree(runtime)


def check_report(report, budgets):
    """Missing coverage or latency is a failure, not a zero-cost run."""
    failures = []
    if not report.get("paging_start") or not report.get("paging_end"):
        failures.append("missing host paging coverage")
    elif report["paging_start"] != report["paging_end"]:
        failures.append("host paging invalidates the performance comparison")
    limits = (
        budgets["ci_software_pss_mib"]
        if report.get("budget_tier") == "software"
        else budgets["ci_pss_mib"]
    )
    for name, ceiling in limits.items():
        sample = report.get("scenarios", {}).get(name, {})
        memory = sample.get("pss_mib", {}).get("shell")
        coverage = sample.get("memory_valid_samples", {}).get("pss", {}).get("shell", 0)
        count = len(sample.get("samples", []))
        if not memory or not count or coverage != count:
            failures.append(name + ": incomplete shell PSS coverage")
        swap = sample.get("swap_pss_mib", {}).get("shell")
        swap_coverage = (
            sample.get("memory_valid_samples", {}).get("swap_pss", {}).get("shell", 0)
        )
        if not swap or not count or swap_coverage != count:
            failures.append(name + ": incomplete shell SwapPss coverage")
        elif swap["max"] > 0:
            failures.append(name + ": swapped shell memory invalidates the comparison")
        if memory and memory["max"] > ceiling:
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
    parser.add_argument(
        "--helper-binary",
        type=Path,
        help="Prebuilt native helper matching the selected immutable revision",
    )
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--labwc", default="labwc")
    parser.add_argument(
        "--parent-renderer",
        choices=("auto", "gles2", "pixman"),
        default="auto",
        help="Outer headless renderer; auto permits runners without a DRM render node",
    )
    parser.add_argument("--labwc-libraries", type=Path)
    parser.add_argument("--wlr-randr", default="wlr-randr")
    parser.add_argument("--sample-seconds", type=float, default=5)
    parser.add_argument("--idle-seconds", type=float, default=30)
    parser.add_argument(
        "--full-idle-windows",
        action="store_true",
        help="Require and record per-phase 60-second paging windows for N0002",
    )
    parser.add_argument("--repeats", type=int, default=10)
    parser.add_argument("--quick", action="store_true")
    parser.add_argument("--latency-only", action="store_true")
    parser.add_argument(
        "--wallpaper-frame-smoke",
        action="store_true",
        help="Exercise selection-to-shell-frame only; never reports a whole-desktop result",
    )
    parser.add_argument(
        "--palette-probes",
        type=Path,
        help="Directory containing prebuilt gtk-palette and qt-palette developer witnesses",
    )
    parser.add_argument(
        "--lock-frame-smoke",
        action="store_true",
        help="Start actual Hyprlock only inside the disposable nested compositor",
    )
    parser.add_argument("--no-latency", action="store_true")
    parser.add_argument(
        "--supervised-shell",
        action="store_true",
        help="Exercise the selected revision's real supervisor in the disposable session",
    )
    parser.add_argument(
        "--revision", help="Immutable git revision for the before capture"
    )
    parser.add_argument("--keep-fixtures", type=Path)
    parser.add_argument(
        "--control",
        action="store_true",
        help="Disable only this speed pass in the copied shell for a same-source comparison",
    )
    parser.add_argument("--check-budgets", action="store_true")
    parser.add_argument(
        "--budget-tier",
        choices=("ci", "software"),
        default="ci",
        help="Explicit software-rendering CI tier; hardware reference budgets stay separate",
    )
    parser.add_argument(
        "--adaptive-allocator",
        action="store_true",
        help="Control capture without the shell large-buffer allocator policy",
    )
    parser.add_argument(
        "--allocator-mmap-threshold",
        type=int,
        help="Experimental glibc large-buffer threshold in bytes",
    )
    parser.add_argument(
        "--orient-binary", type=Path, help="Pinned native engine for fixture helpers"
    )
    args = parser.parse_args()
    if args.palette_probes and not args.wallpaper_frame_smoke:
        parser.error("--palette-probes requires --wallpaper-frame-smoke")
    if args.lock_frame_smoke and not args.palette_probes:
        parser.error("--lock-frame-smoke requires --palette-probes")
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
