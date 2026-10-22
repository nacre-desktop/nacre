#!/usr/bin/env python3
"""Seeded, isolated overnight shell exercise. Artifacts stay under /tmp."""

import argparse
import asyncio
from collections import Counter, defaultdict, deque
import csv
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import random
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time

import performance

ROOT = Path(__file__).resolve().parents[1]
ACTIONS = (
    "launcher",
    "close",
    "dashboard",
    "settings",
    "detach",
    "dock",
    "notifications",
    "notification_clear",
    "clipboard",
    "workspace",
    "window",
    "wallpaper",
    "palette",
)
FIELDS = (
    "seconds",
    "phase",
    "role",
    "pid",
    "start",
    "pss_mib",
    "uss_mib",
    "swap_pss_mib",
    "fds",
    "threads",
    "children",
    "state",
    "memory_error",
    "qml_warnings",
    "shell_exits",
    "host_pswpin",
    "host_pswpout",
)
SETTINGS = ("appearance", "desktop", "displays", "sound", "notifications")
WARNING = re.compile(r"(?:WARN|ERROR|qml:|ReferenceError|TypeError|Binding loop)", re.I)
HELPER_EVENT = re.compile(r"SOAK_HELPER_(START|EXIT) ([\w-]+)(?: (-?\d+) (\d+))?")
WAYLAND_LINE = re.compile(rb"^\s*\[(?:\d+\.\d+|\d{2}:\d{2}:\d{2}(?:\.\d+)?)\]")


class TraceTail:
    """Bound protocol storage while forwarding ordinary stderr unchanged."""

    def __init__(self, budget=4 * 1048576):
        self.budget = budget
        self.lines = deque()
        self.size = 0
        self.dropped = 0
        self.lock = threading.Lock()
        self.thread = None

    def append(self, line):
        line = line[: min(4096, self.budget)]
        with self.lock:
            self.lines.append(line)
            self.size += len(line)
            while self.size > self.budget:
                self.dropped += 1
                self.size -= len(self.lines.popleft())

    def read(self, stream, log):
        with log.open("ab", buffering=0) as output:
            for line in stream:
                self.append(line)
                if not WAYLAND_LINE.match(line):
                    output.write(line)

    def dump(self, path):
        with self.lock:
            data = b"".join(self.lines)
        path.write_bytes(data)


def load_session():
    spec = importlib.util.spec_from_file_location(
        "soak_session", ROOT / "tools/performance-session.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


async def notify_image(index, transient):
    """Supplemental raw-image lifetime probe; never used by the matched baseline."""
    from dbus_next import Message, MessageType, Variant
    from dbus_next.aio import MessageBus

    bus = await MessageBus().connect()
    try:
        reply = await asyncio.wait_for(
            bus.call(
                Message(
                    destination="org.freedesktop.Notifications",
                    path="/org/freedesktop/Notifications",
                    interface="org.freedesktop.Notifications",
                    member="Notify",
                    signature="susssasa{sv}i",
                    body=[
                        "Soak Demo",
                        0,
                        "",
                        f"Image demo {index}",
                        "Public image lifetime probe",
                        [],
                        {
                            "transient": Variant("b", transient),
                            "image-data": Variant(
                                "(iiibiiay)",
                                [
                                    64,
                                    64,
                                    256,
                                    True,
                                    8,
                                    4,
                                    bytes([80, 120, 180, 255]) * 4096,
                                ],
                            ),
                        },
                        3000,
                    ],
                )
            ),
            timeout=15,
        )
        if reply.message_type == MessageType.ERROR:
            raise RuntimeError("image notification rejected: " + str(reply.error_name))
    finally:
        bus.disconnect()


def source_state():
    """Identify uncommitted test candidates without including ignored evidence."""
    paths = (
        subprocess.check_output(
            ["git", "ls-files", "-co", "--exclude-standard", "-z"], cwd=ROOT
        )
        .decode()
        .split("\0")
    )
    digest = hashlib.sha256()
    for name in sorted(set(paths) - {""}):
        path = ROOT / name
        if not path.is_file():
            continue
        digest.update(name.encode() + b"\0")
        digest.update(hashlib.sha256(path.read_bytes()).digest())
    return {
        "dirty": bool(
            subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT)
        ),
        "files_sha256": digest.hexdigest(),
    }


def slope(points):
    """Least squares units/hour and R squared; reject missing coverage."""
    if len(points) < 5:
        return None
    xs, ys = zip(*points, strict=True)
    xm, ym = sum(xs) / len(xs), sum(ys) / len(ys)
    xx = sum((x - xm) ** 2 for x in xs)
    if not xx:
        return None
    beta = sum((x - xm) * (y - ym) for x, y in points) / xx
    total = sum((y - ym) ** 2 for y in ys)
    error = sum((y - ym - beta * (x - xm)) ** 2 for x, y in points)
    return {
        "per_hour": beta * 3600,
        "r_squared": 1 - error / total if total else 0,
        "first": ys[0],
        "last": ys[-1],
        "samples": len(points),
    }


def analyze(rows):
    groups = defaultdict(list)
    for row in rows:
        if row["phase"] == "measured":
            sample = dict(row)
            sample["combined_pss_mib"] = (
                float(row["pss_mib"]) + float(row["swap_pss_mib"])
                if row.get("pss_mib") not in (None, "")
                and row.get("swap_pss_mib") not in (None, "")
                else None
            )
            groups[(row["role"], row["pid"], row["start"])].append(sample)
    trends, flags = [], []
    for identity, samples in groups.items():
        entry = {
            "role": identity[0],
            "pid": identity[1],
            "start": identity[2],
            "metrics": {},
        }
        for metric in (
            "pss_mib",
            "uss_mib",
            "combined_pss_mib",
            "fds",
            "threads",
            "children",
        ):
            points = [
                (float(row["seconds"]), float(row[metric]))
                for row in samples
                if row[metric] not in (None, "")
            ]
            result = slope(points)
            entry["metrics"][metric] = result
            threshold = 5 if metric.endswith("mib") else 0.5
            if result and result["per_hour"] > threshold and result["r_squared"] >= 0.6:
                flags.append(
                    {
                        "role": identity[0],
                        "pid": identity[1],
                        "metric": metric,
                        **result,
                    }
                )
        trends.append(entry)
    lifetimes = defaultdict(set)
    for row in rows:
        if row["role"] not in ("total", "shell"):
            lifetimes[row["role"]].add((row["pid"], row["start"]))
    return {
        "trends": trends,
        "growth_flags": flags,
        "observed_helper_lifetimes": {
            role: len(values) for role, values in lifetimes.items()
        },
    }


def prepare(directory):
    """Copy public sources, never the owner's installed configs or account data."""
    from PIL import Image

    session = load_session()
    home, shell = directory / "home", directory / "shell"
    shutil.copytree(ROOT / "nacre/shell", shell)
    tools = home / ".local/share/nacre/shell/tools"
    shutil.copytree(
        ROOT / "nacre/shell-tools",
        tools,
        ignore=shutil.ignore_patterns("tests", "__pycache__"),
    )
    # QML dispatches through the native runtime after the helper migration.
    helper = Path(
        os.environ.get("NACRE_HELPER_BINARY", ROOT / "target/release/nacre-helper")
    ).resolve(strict=True)
    shutil.copy2(helper, tools / "nacre-helper")
    (tools / "branding").symlink_to(ROOT / "nacre/branding")
    runtime = home / ".local/share/nacre/palette-runtime/venv"
    runtime.parent.mkdir(parents=True)
    runtime.symlink_to(sys.prefix)
    for name in (
        ".config/nacre",
        ".cache",
        ".local/state/nacre/wallpaper",
        ".local/share/applications",
        "Pictures/Wallpapers",
    ):
        (home / name).mkdir(parents=True, exist_ok=True)
    for i in range(3):
        Image.new("RGB", (1280, 720), ((40 + i * 70), 80, (180 - i * 40))).save(
            home / f"Pictures/Wallpapers/demo-{i}.png"
        )
    wallpapers = home / "Pictures/Wallpapers"
    shutil.copyfile(wallpapers / "demo-0.png", wallpapers / "static.png")
    for i in range(12):
        (home / f".local/share/applications/soak-demo-{i:02d}.desktop").write_text(
            f"[Desktop Entry]\nType=Application\nName=Demo {i:02d}\nExec=true\nIcon=utilities-terminal\nCategories=Utility;\n"
        )
    config = home / ".config/nacre"
    (config / "welcome.json").write_text(
        json.dumps({"version": 1, "showAtLogin": False})
    )
    (config / "wallpaper.json").write_text(
        json.dumps(
            {"rotationEnabled": False, "postHook": True, "directory": str(wallpapers)}
        )
    )
    (config / "launcher.json").write_text(
        json.dumps({"favorites": [f"soak-demo-{i:02d}" for i in range(12)]})
    )
    fixture = session.fixture(home, wallpapers / "static.png")
    state = home / ".local/state/nacre"
    (state / "presentation.json").write_text(json.dumps(fixture["presentation"]))
    (state / "wallpaper/media.json").write_text(json.dumps(fixture["media"]))
    for name in ("path", "last"):
        (state / f"wallpaper/{name}.txt").write_text(fixture["media"]["poster"])
    # Exit telemetry is confined to staged QML, without changing lifetimes.
    helpers = shell / "services/NacreHelpers.qml"
    source = (
        helpers.read_text()
        .replace(
            "        running: true\n",
            '        running: true\n        onStarted: console.info("SOAK_HELPER_START helper-service")\n',
            1,
        )
        .replace(
            "        onExited: {",
            '        onExited: (code, status) => {\n            console.info("SOAK_HELPER_EXIT helper-service " + code + " " + status);',
            1,
        )
    )
    helpers.write_text(source)
    watcher = shell / "services/NacreWallpapers.qml"
    source = (
        watcher.read_text()
        .replace(
            "        onStarted: watchStable.start()",
            '        onStarted: { watchStable.start(); console.info("SOAK_HELPER_START wallpaper-watch"); }',
        )
        .replace(
            "        onExited: {\n            watchStable.stop();",
            '        onExited: (code, status) => {\n            console.info("SOAK_HELPER_EXIT wallpaper-watch " + code + " " + status);\n            watchStable.stop();',
        )
    )
    watcher.write_text(source)
    notices = shell / "services/NacreNotifs.qml"
    notices.write_text(
        notices.read_text()
        .replace(
            "                historyReady: root.historyReady,",
            "                tracked: serverLoader.item.trackedNotifications.values.length,\n                liveReferences: root.list.filter(entry => entry.notification).length,\n                images: root.list.filter(entry => entry.image).length,\n                dismissedKeys: Object.keys(root.dismissedKeys).length,\n                historyReady: root.historyReady,",
            1,
        )
    )
    movie = wallpapers / "video.mp4"
    if shutil.which("ffmpeg"):
        subprocess.run(
            [
                "ffmpeg",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=640x360:rate=15",
                "-t",
                "3",
                "-c:v",
                "libx264",
                "-threads",
                "2",
                "-preset",
                "ultrafast",
                "-pix_fmt",
                "yuv420p",
                str(movie),
            ],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
    (directory / "window.qml").write_text(
        'import QtQuick\nimport Quickshell\nFloatingWindow { title: "Soak Demo"; visible: true; implicitWidth: 320; implicitHeight: 200; Rectangle { anchors.fill: parent; color: "#304060" } }\n'
    )
    return home


def sandbox(directory, parent, arguments):
    """Expose only the parent Wayland endpoint, not its bus or compositor IPC."""
    home = Path.home()
    command = [
        "bwrap",
        "--ro-bind",
        "/",
        "/",
        "--unshare-pid",
        "--unshare-net",
        "--die-with-parent",
        "--proc",
        "/proc",
        "--dev-bind",
        "/dev",
        "/dev",
        "--tmpfs",
        str(home),
        "--tmpfs",
        "/tmp",
        "--tmpfs",
        "/run/user",
        "--tmpfs",
        "/run/dbus",
        "--bind",
        str(directory),
        str(directory),
        "--ro-bind",
        str(ROOT),
        str(ROOT),
        "--ro-bind",
        str(parent),
        "/tmp/soak-parent.sock",
    ]
    prefix = Path(sys.prefix)
    if (
        prefix != ROOT
        and not prefix.is_relative_to(ROOT)
        and prefix.is_relative_to(home)
    ):
        command += ["--ro-bind", str(prefix), str(prefix)]
    return command + [
        "dbus-run-session",
        "--",
        sys.executable,
        str(ROOT / "tools/soak.py"),
        "--worker",
        *arguments,
    ]


def environment(directory, backend):
    home = directory / "home"
    env = {
        "PATH": f"{sys.prefix}/bin:/usr/bin:/bin",
        "LANG": "C.UTF-8",
        "HOME": str(home),
        "XDG_RUNTIME_DIR": str(directory / "run"),
        "XDG_CONFIG_HOME": str(home / ".config"),
        "XDG_CACHE_HOME": str(home / ".cache"),
        "XDG_DATA_HOME": str(home / ".local/share"),
        "XDG_STATE_HOME": str(home / ".local/state"),
        "XDG_DATA_DIRS": "/usr/local/share:/usr/share",
        "XDG_CURRENT_DESKTOP": backend,
        "XDG_SESSION_TYPE": "wayland",
        "QT_QPA_PLATFORM": "wayland",
        # Match public UWSM toolkit policy; private qt6ct state stays hidden.
        "QT_QPA_PLATFORMTHEME": "qt6ct",
        "QT_WAYLAND_DISABLE_WINDOWDECORATION": "1",
        "WAYLAND_DISPLAY": "/tmp/soak-parent.sock",
        "NACRE_COMPOSITOR": backend,
        "NACRE_QS_BIN": "/usr/bin/quickshell",
        "NACRE_QS_CONFIG": str(directory / "shell/shell.qml"),
        "PYTHONPATH": str(ROOT / "nacre/shell-cli/src"),
    }
    # The bus was created inside the namespace; no host address is inherited.
    if os.environ.get("DBUS_SESSION_BUS_ADDRESS"):
        env["DBUS_SESSION_BUS_ADDRESS"] = os.environ["DBUS_SESSION_BUS_ADDRESS"]
    if (directory / "orient").is_file():
        env["NACRE_ORIENT_BINARY"] = str(directory / "orient")
    return env


MAX_PEARLS = 4


class PearlWorkloadError(RuntimeError):
    """A failed handoff must not turn a bounded soak into window accumulation."""


def read_pearls(ipc):
    entries = json.loads(ipc("pearls", "state"))
    if not isinstance(entries, list) or any(
        not isinstance(entry, dict) or not isinstance(entry.get("key"), str)
        for entry in entries
    ):
        raise PearlWorkloadError("Invalid pearl state")
    return entries


def check_pearl_bound(entries):
    if len(entries) > MAX_PEARLS:
        raise PearlWorkloadError(
            f"Pearl workload exceeded its limit: {len(entries)} > {MAX_PEARLS}"
        )


def dock_pearls(ipc, wait_for, recover=False):
    entries = read_pearls(ipc)
    recovered = 0
    for entry in entries:
        key = entry["key"]

        def removed(key=key):
            return all(value["key"] != key for value in read_pearls(ipc))

        # These handoffs share one destination panel. Await each transaction
        # before restoring another state or detaching the destination again.
        ipc("pearls", "dock", key)
        try:
            wait_for(removed, "pearl docking", timeout=2)
        except RuntimeError as error:
            if removed():
                continue
            if not recover:
                raise PearlWorkloadError(
                    "Docking did not finish; pearl remains"
                ) from error
            ipc("pearls", "close", key)
            try:
                wait_for(removed, "pearl cleanup", timeout=2)
            except RuntimeError as cleanup_error:
                raise PearlWorkloadError(
                    "Pearl cleanup did not finish"
                ) from cleanup_error
            recovered += 1
    return recovered


def detach_pearl(ipc, wait_for, recover=False):
    entries = read_pearls(ipc)
    check_pearl_bound(entries)
    recovered = 0
    if len(entries) >= MAX_PEARLS:
        recovered = dock_pearls(ipc, wait_for, recover)
    ipc("nacre", "launcher", "Demo")
    ipc("pearls", "detach")
    check_pearl_bound(read_pearls(ipc))
    return recovered


def renderer_command(directory):
    """Use the real renderer launcher so allocator/toolkit policy cannot drift."""
    return [
        "bash",
        str(ROOT / "nacre/shell-tools/launch.sh"),
        "-n",
        "-p",
        str(directory / "shell"),
    ]


def measure(pid):
    row = performance.process(pid)
    if not row:
        return None
    row.update(performance.memory(pid))
    path = Path("/proc") / str(pid)
    try:
        status = dict(
            line.split(":", 1) for line in (path / "status").read_text().splitlines()
        )
        row.update(
            fds=len(list((path / "fd").iterdir())),
            threads=int(status["Threads"]),
            state=status["State"].strip(),
        )
    except (OSError, ValueError, KeyError):
        return None
    current = performance.process(pid)
    return row if current and current["start"] == row["start"] else None


class WarningReader:
    def __init__(self, path):
        self.path, self.offset, self.counts = path, 0, Counter()
        self.helper_events = Counter()

    def read(self):
        with self.path.open(errors="replace") as log:
            log.seek(self.offset)
            for line in log:
                if match := HELPER_EVENT.search(line):
                    key = " ".join(v for v in match.groups() if v is not None)
                    self.helper_events[key] += 1
                    continue
                if WARNING.search(line):
                    # Collapse timestamps, volatile addresses and disposable paths.
                    value = re.sub(
                        r"0x[0-9a-f]+|\b\d{2}:\d{2}:\d{2}(?:\.\d+)?\b",
                        "#",
                        line.strip(),
                    )
                    value = value.replace(str(self.path.parent), "$SOAK")
                    self.counts[value[:500]] += 1
            self.offset = log.tell()
        return sum(self.counts.values())


def worker(args, directory):
    session = load_session()
    env = environment(directory, args.backend)
    # Hyprland signatures are long: leave room within Unix sockets' 108-byte limit.
    runtime = Path(tempfile.mkdtemp(prefix="ns-"))
    env["XDG_RUNTIME_DIR"] = str(runtime)
    processes, events, rows = [], [], []
    rng = random.Random(args.seed)
    started = time.monotonic()
    pid = None
    exits = 0
    actions = Counter()
    failures = Counter()
    pearl_peak = 0
    pearl_recoveries = 0
    windows = []
    traces = []
    last_ipc_success = started
    warnings = WarningReader(directory / "shell.log")

    def start(command, name, extra=None):
        traced = name == "shell" and args.wayland_debug
        child_env = extra or env
        if traced:
            child_env = dict(child_env, WAYLAND_DEBUG="client")
        with (directory / f"{name}.log").open("a") as log:
            process = subprocess.Popen(
                command,
                env=child_env,
                stdout=log,
                stderr=subprocess.PIPE if traced else log,
                start_new_session=True,
            )
        processes.append(process)
        if traced:
            tail = TraceTail()
            tail.thread = threading.Thread(
                target=tail.read,
                args=(process.stderr, directory / "shell.log"),
                daemon=True,
            )
            tail.thread.start()
            traces.append(tail)
        return process

    def run(command, input=None):
        result = subprocess.run(
            command, env=env, input=input, capture_output=True, text=True, timeout=15
        )
        if result.returncode:
            raise RuntimeError(
                f"command exit {result.returncode}: {result.stdout[-400:]} {result.stderr[-400:]}"
            )
        return result.stdout.strip()

    def ipc(*words):
        nonlocal last_ipc_success
        result = run(["quickshell", "ipc", "--pid", str(pid), "call", *map(str, words)])
        last_ipc_success = time.monotonic()
        return result

    def event(kind, **values):
        item = {"seconds": round(time.monotonic() - started, 3), "kind": kind, **values}
        events.append(item)
        with (directory / "events.jsonl").open("a") as stream:
            stream.write(json.dumps(item) + "\n")

    def sample(writer, stream):
        nonlocal pearl_peak
        elapsed = time.monotonic() - started
        phase = "warmup" if elapsed < args.warmup else "measured"
        tree = performance.snapshot(pid, detached=False)
        # PID namespace contains only this test. Include daemonized helpers too.
        for path in Path("/proc").iterdir():
            if not path.name.isdigit():
                continue
            info = performance.process(path.name)
            if info and any(
                str(directory / "home").encode() in arg for arg in info["args"]
            ):
                tree[info["pid"]] = info
        warning_count = warnings.read()
        paging = session.paging()
        sampled = []
        for key in sorted(tree):
            info = measure(key)
            if not info:
                continue
            role = (
                "shell"
                if key == pid
                else next(
                    (
                        Path(arg.decode(errors="replace")).name
                        for arg in info["args"]
                        if arg.endswith(b".py")
                    ),
                    info["name"],
                )
            )
            row = {
                "seconds": round(elapsed, 3),
                "phase": phase,
                "role": role,
                "pid": key,
                "start": info["start"],
                "fds": info["fds"],
                "threads": info["threads"],
                "children": sum(value["ppid"] == key for value in tree.values()),
                "state": info["state"],
                "memory_error": info["memory_error"],
                "qml_warnings": warning_count if key == pid else None,
                "shell_exits": exits,
                "host_pswpin": paging["pswpin"],
                "host_pswpout": paging["pswpout"],
            }
            for metric in ("pss", "uss", "swap_pss"):
                row[metric + "_mib"] = (
                    info[metric] / 1048576 if info[metric] is not None else None
                )
            rows.append(row)
            sampled.append(row)
            writer.writerow(row)
        aggregate = {
            "seconds": round(elapsed, 3),
            "phase": phase,
            "role": "total",
            "pid": 0,
            "start": 0,
            "children": max(0, len(sampled) - 1),
            "qml_warnings": warning_count,
            "shell_exits": exits,
        }
        for metric in ("pss_mib", "uss_mib", "swap_pss_mib", "fds", "threads"):
            values = [row[metric] for row in sampled]
            aggregate[metric] = (
                sum(values) if values and all(v is not None for v in values) else None
            )
        rows.append(aggregate)
        writer.writerow(aggregate)
        stream.flush()
        try:
            counts = json.loads(ipc("notifs", "counts"))
            counts.pop("timers", None)
            event("notification_counts", **counts)
        except (OSError, RuntimeError, ValueError, subprocess.TimeoutExpired) as error:
            event("probe_failure", error=str(error)[-500:])
        pearls = read_pearls(ipc)
        pearl_peak = max(pearl_peak, len(pearls))
        event("pearl_counts", count=len(pearls))
        check_pearl_bound(pearls)
        shell_sample = next((row for row in sampled if row["role"] == "shell"), {})
        progress = {
            "backend": args.backend,
            "elapsed_minutes": round(elapsed / 60, 1),
            "phase": phase,
            "shell_pss_mib": shell_sample.get("pss_mib"),
            "shell_fds": shell_sample.get("fds"),
            "shell_threads": shell_sample.get("threads"),
            "processes": len(sampled),
            "warnings": warning_count,
            "actions": sum(actions.values()),
            "failures": sum(failures.values()),
            "pearl_count": len(pearls),
            "pearl_peak": pearl_peak,
            "pearl_recoveries": pearl_recoveries,
        }
        (directory / "progress.json").write_text(json.dumps(progress) + "\n")
        print(
            json.dumps(progress),
            flush=True,
        )

    def action(name):
        nonlocal pearl_peak, pearl_recoveries
        if name == "launcher":
            ipc(
                "nacre",
                "launcher",
                rng.choice(("", "Demo", "term", "not-a-match", "Demo 0")),
            )
        elif name == "close":
            ipc("nacre", "close")
            ipc("settingsApp", "close")
        elif name == "dashboard":
            ipc("nacre", "tab", rng.randrange(5))
        elif name == "settings":
            ipc("settingsApp", "open", rng.choice(SETTINGS))
        elif name == "detach":
            recovered = detach_pearl(ipc, session.wait_for, args.recover_docks)
            pearl_peak = max(pearl_peak, len(read_pearls(ipc)))
            pearl_recoveries += recovered
            if recovered:
                event("pearl_dock_recovery", closed=recovered)
        elif name == "dock":
            pearl_peak = max(pearl_peak, len(read_pearls(ipc)))
            recovered = dock_pearls(ipc, session.wait_for, args.recover_docks)
            pearl_recoveries += recovered
            if recovered:
                event("pearl_dock_recovery", closed=recovered)
        elif name == "wallpaper":
            choices = sorted((directory / "home/Pictures/Wallpapers").glob("demo-*"))
            movie = directory / "home/Pictures/Wallpapers/video.mp4"
            if movie.exists():
                choices.append(movie)
            ipc("wallpaper", "set", str(rng.choice(choices)))
        elif name == "palette":
            run(
                [
                    sys.executable,
                    "-m",
                    "nacre_shell",
                    "scheme",
                    "set",
                    "--mode",
                    rng.choice(("dark", "light")),
                    "--accent",
                    rng.choice(("4c86ce", "c06080", "60a080")),
                ]
            )
        elif name == "notifications":
            transient = rng.choice((True, False))
            for i in range(rng.randint(2, 6)):
                if args.notification_images:
                    asyncio.run(notify_image(i, transient))
                    continue
                run(
                    [
                        "notify-send",
                        *(["-h", "int:transient:1"] if transient else []),
                        "-a",
                        "Soak Demo",
                        "-t",
                        "3000",
                        f"Demo {i}",
                        "Public test notification",
                    ]
                )
        elif name == "notification_clear":
            ipc("notifs", "clearHistory")
        elif name == "clipboard":
            run(["wl-copy", "--foreground", "--clear"])
            # A bounded owner avoids leaving one wl-copy daemon per event.
            process = start(
                ["wl-copy", "--foreground", f"Public demo {rng.randrange(100)}"],
                "clipboard",
            )
            time.sleep(0.2)
            process.terminate()
            process.wait(timeout=5)
        elif name == "workspace":
            if args.backend == "niri":
                run(
                    [
                        "/usr/bin/niri",
                        "msg",
                        "action",
                        "focus-workspace",
                        str(rng.randint(1, 3)),
                    ]
                )
            else:
                run(
                    [
                        "hyprctl",
                        "eval",
                        'hl.dispatch(hl.dsp.focus({workspace="'
                        + str(rng.randint(1, 3))
                        + '",on_current_monitor=true}))',
                    ]
                )
        elif name == "window":
            if windows:
                process = windows.pop()
                process.terminate()
                process.wait(timeout=5)
            else:
                windows.append(
                    start(
                        ["quickshell", "-n", "-p", str(directory / "window.qml")],
                        "window",
                    )
                )
        elif name == "scale":
            outputs = json.loads(run(["/usr/bin/niri", "msg", "--json", "outputs"]))
            for output in outputs:
                run(
                    [
                        "/usr/bin/niri",
                        "msg",
                        "output",
                        output,
                        "scale",
                        str(rng.choice((1, 1.25, 1.5))),
                    ]
                )

    failure = None
    try:
        if args.backend == "niri":
            config = directory / "niri.kdl"
            config.write_text(
                'input { keyboard { xkb { layout "us"; }; }; };\noutput "winit" { mode "1280x720"; scale 1; };\nprefer-no-csd\nwindow-rule { match app-id="^org[.]quickshell$"; open-floating true; }\n'
            )
            compositor = start(["/usr/bin/niri", "-c", str(config)], "compositor")
            socket = session.wait_for(
                lambda: next(iter(runtime.glob("niri.*.sock")), None),
                "private niri socket",
            )
            env["NIRI_SOCKET"] = str(socket)
        else:
            config = directory / "hyprland.lua"
            config.write_text(
                'hl.monitor({output="",mode="1280x720@60",position="auto",scale=1})\nhl.window_rule({name="soak",match={class="^org[.]quickshell$"},float=true})\nhl.config({animations={enabled=false},misc={disable_hyprland_logo=true,disable_splash_rendering=true}})\n'
            )
            compositor = start(
                ["Hyprland", "-c", str(config)],
                "compositor",
                dict(env, AQ_DRM_DEVICES="/dev/null", HYPRLAND_NO_CRASHREPORTER="1"),
            )
            socket = session.wait_for(
                lambda: next(iter(runtime.glob("hypr/*/.socket.sock")), None),
                "private Hyprland socket",
            )
            env["HYPRLAND_INSTANCE_SIGNATURE"] = socket.parent.name
            if args.hyprland_headless:
                run(["hyprctl", "output", "create", "headless"])
                monitors = json.loads(run(["hyprctl", "-j", "monitors", "all"]))
                for monitor in monitors:
                    if monitor["name"].startswith("WAYLAND-"):
                        run(
                            [
                                "hyprctl",
                                "eval",
                                "hl.monitor({output="
                                + json.dumps(monitor["name"])
                                + ",disabled=true})",
                            ]
                        )
        display = session.wait_for(
            lambda: next((p for p in runtime.glob("wayland-*") if p.is_socket()), None),
            "private Wayland socket",
        )
        env["WAYLAND_DISPLAY"] = str(display)
        if args.render_loop != "auto":
            env["QSG_RENDER_LOOP"] = args.render_loop
        # Display and signature were just resolved in this private namespace.
        # Do not rediscover another instance through a supervisor startup query.
        shell = start(renderer_command(directory), "shell")
        pid = shell.pid

        def ready():
            if shell.poll() is not None:
                raise RuntimeError(f"shell exited during startup: {shell.returncode}")
            try:
                return json.loads(ipc("nacre", "state"))
            except (RuntimeError, ValueError, subprocess.TimeoutExpired):
                return None

        session.wait_for(ready, "shell IPC", timeout=60)
        # Timing starts only once startup and IPC succeeded.
        started = time.monotonic()
        last_ipc_success = started
        event("ready", shell_pid=pid, compositor_pid=compositor.pid)
        names = args.actions or list(ACTIONS)
        next_action, next_sample, next_scale = started, started, started
        with (directory / "samples.csv").open("w", newline="") as stream:
            writer = csv.DictWriter(stream, fieldnames=FIELDS)
            writer.writeheader()
            while time.monotonic() - started <= args.warmup + args.hours * 3600:
                now = time.monotonic()
                if now - last_ipc_success > 120:
                    raise RuntimeError(
                        "shell IPC unresponsive for over 120 seconds; workload cannot continue"
                    )
                for process, role in ((shell, "shell"), (compositor, "compositor")):
                    if process.poll() is not None:
                        exits += role == "shell"
                        event("exit", role=role, returncode=process.returncode)
                        raise RuntimeError(
                            f"{role} exited: {process.returncode}; no restart hides a crash"
                        )
                if now >= next_sample:
                    sample(writer, stream)
                    next_sample += args.interval
                if now >= next_action:
                    name = rng.choice(names)
                    if (
                        args.backend == "niri"
                        and not args.actions
                        and now >= next_scale
                    ):
                        name, next_scale = "scale", now + 300
                    actions[name] += 1
                    event("action", action=name)
                    try:
                        action(name)
                    except (
                        subprocess.TimeoutExpired,
                        OSError,
                        RuntimeError,
                        ValueError,
                    ) as error:
                        failures[name] += 1
                        event("action_failure", action=name, error=str(error)[-500:])
                        if isinstance(error, PearlWorkloadError):
                            raise
                    processes[:] = [p for p in processes if p.poll() is None]
                    next_action = now + args.action_interval
                time.sleep(min(0.5, max(0.01, next_sample - time.monotonic())))
            sample(writer, stream)
    except KeyboardInterrupt:
        failure = "interrupted"
        event("failure", error=failure)
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        failure = str(error)
        event("failure", error=failure)
    finally:
        if warnings.path.exists():
            warnings.read()
        # Only groups launched by this driver inside its private PID namespace.
        for process in reversed(processes):
            if process.poll() is None:
                try:
                    os.killpg(process.pid, signal.SIGTERM)
                    process.wait(timeout=5)
                except (ProcessLookupError, subprocess.TimeoutExpired):
                    if process.poll() is None:
                        os.killpg(process.pid, signal.SIGKILL)
                        process.wait(timeout=5)
        for trace in traces:
            trace.thread.join(timeout=1)
            trace.dump(directory / "wayland-tail.log")
        duration_complete = any(
            row["role"] == "shell" and row["seconds"] >= args.warmup + args.hours * 3600
            for row in rows
        )
        probe_failures = sum(e["kind"] == "probe_failure" for e in events)
        memory_workload_healthy = (
            duration_complete
            and failure is None
            and not failures
            and not probe_failures
            and pearl_peak <= MAX_PEARLS
        )
        workload_healthy = memory_workload_healthy and not pearl_recoveries
        report = {
            "backend": args.backend,
            "seed": args.seed,
            "action_selection": args.actions,
            "notification_images": args.notification_images,
            "source_revision": (directory / "revision").read_text().strip(),
            "source_state": json.loads((directory / "source-state.json").read_text()),
            "busy_host": True,
            "duration_complete": duration_complete,
            "workload_healthy": workload_healthy,
            "memory_workload_healthy": memory_workload_healthy,
            "pearl_limit": MAX_PEARLS,
            "pearl_peak": pearl_peak,
            "pearl_recoveries": pearl_recoveries,
            "recover_docks": args.recover_docks,
            "probe_failures": probe_failures,
            "runtime_policy": {
                name: env.get(name, "auto")
                for name in (
                    "QT_QPA_PLATFORMTHEME",
                    "QT_WAYLAND_DISABLE_WINDOWDECORATION",
                    "QSG_RENDER_LOOP",
                )
            },
            "wayland_trace": bool(traces),
            "renderer_launcher": "nacre/shell-tools/launch.sh",
            "hyprland_headless": args.hyprland_headless,
            "requested_hours": args.hours,
            "warmup_seconds": args.warmup,
            "elapsed_seconds": time.monotonic() - started,
            "sample_interval": args.interval,
            "failure": failure,
            "shell_exits": exits,
            "actions": dict(actions),
            "action_failures": dict(failures),
            "helper_events": dict(warnings.helper_events),
            "zombie_observations": sum(
                row.get("state", "").startswith("Z") for row in rows
            ),
            "warnings": [
                {"message": k, "count": v} for k, v in warnings.counts.most_common()
            ],
            **analyze(rows),
        }
        (directory / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    return int(not workload_healthy)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--backend", choices=("niri", "hyprland", "both"), default="both"
    )
    parser.add_argument("--hours", type=float, default=3)
    parser.add_argument("--warmup", type=float, default=600)
    parser.add_argument("--interval", type=float, default=60)
    parser.add_argument("--action-interval", type=float, default=5)
    parser.add_argument("--seed", type=int, default=20261013)
    parser.add_argument(
        "--recover-docks",
        action="store_true",
        help="bounded memory control: close fixture pearls after a dock timeout; "
        "record recovery and mark the full workload degraded",
    )
    parser.add_argument(
        "--actions",
        nargs="+",
        choices=ACTIONS,
        help="supplemental isolated-action probe; omit for matched workload",
    )
    parser.add_argument(
        "--notification-images",
        action="store_true",
        help="supplemental raw-image notifications (requires dbus-next)",
    )
    parser.add_argument("--output", type=Path)
    parser.add_argument("--orient-binary", type=Path)
    parser.add_argument(
        "--hyprland-headless",
        action="store_true",
        help="use a virtual Hyprland output to isolate nested Wayland presentation",
    )
    parser.add_argument(
        "--wayland-debug",
        action="store_true",
        help="retain only the last 4 MiB of shell Wayland traffic",
    )
    parser.add_argument(
        "--render-loop",
        choices=("auto", "basic", "threaded"),
        default="auto",
        help="test-session-only Qt render-loop comparison",
    )
    parser.add_argument("--worker", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    for value in (args.hours, args.interval, args.action_interval):
        if not math.isfinite(value) or value <= 0:
            parser.error("durations must be finite and positive")
    if not math.isfinite(args.warmup) or args.warmup < 0:
        parser.error("warmup must be finite and nonnegative")
    if args.worker:
        return worker(args, args.output)
    os.nice(10)
    for tool in ("bwrap", "dbus-run-session", "quickshell", "notify-send", "wl-copy"):
        if not shutil.which(tool):
            parser.error(f"required tool unavailable: {tool}")
    display = Path(os.environ.get("WAYLAND_DISPLAY", ""))
    parent = (
        display
        if display.is_absolute()
        else Path(os.environ.get("XDG_RUNTIME_DIR", "/nonexistent")) / display
    )
    if not parent.is_socket():
        parser.error(
            "a parent Wayland socket is required; never starts a real login session"
        )
    directory = args.output or Path(tempfile.mkdtemp(prefix="nacre-soak-"))
    directory = directory.resolve()
    if not directory.is_relative_to(Path("/tmp")) or directory.is_symlink():
        parser.error("output must be a new directory under /tmp")
    directory.mkdir(mode=0o700, parents=True, exist_ok=True)
    if any(directory.iterdir()):
        parser.error("output directory must be empty")
    revision = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
    ).strip()
    state = source_state()
    result = 0
    for backend in ("niri", "hyprland") if args.backend == "both" else (args.backend,):
        target = directory / backend
        target.mkdir(mode=0o700)
        prepare(target)
        (target / "revision").write_text(revision + "\n")
        (target / "source-state.json").write_text(json.dumps(state) + "\n")
        if args.orient_binary:
            shutil.copyfile(args.orient_binary.resolve(strict=True), target / "orient")
            (target / "orient").chmod(0o755)
        words = [
            "--backend",
            backend,
            "--hours",
            str(args.hours),
            "--warmup",
            str(args.warmup),
            "--interval",
            str(args.interval),
            "--action-interval",
            str(args.action_interval),
            "--seed",
            str(args.seed),
            "--output",
            str(target),
            "--render-loop",
            args.render_loop,
        ]
        if args.wayland_debug:
            words.append("--wayland-debug")
        if args.hyprland_headless:
            words.append("--hyprland-headless")
        if args.notification_images:
            words.append("--notification-images")
        if args.recover_docks:
            words.append("--recover-docks")
        if args.actions:
            words.extend(["--actions", *args.actions])
        # No inherited accounts, injected Qt paths or session activation variables.
        clean = {
            "PATH": "/usr/bin:/bin",
            "LANG": "C.UTF-8",
            "HOME": str(target / "home"),
        }
        process = subprocess.run(sandbox(target, parent, words), env=clean, check=False)
        result |= bool(process.returncode)
        print(f"{backend}: exit {process.returncode}; artifacts: {target}", flush=True)
    return int(result)


if __name__ == "__main__":
    sys.exit(main())
