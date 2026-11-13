#!/usr/bin/env python3
"""One benchmark arm: Noctalia or Nacre in a private nested session, sampled.

Topology: private D-Bus, headless labwc parent, nested stock niri at
1920x1080@60, private PipeWire with null devices, fake UPower/BlueZ on the bus,
then the shell under test. Four process trees are sampled once a second from
/proc: shell, daemon (Nacre only), niri and labwc. CPU uses per-thread
schedstat nanoseconds, so a 0.01 % idle number is real, not a tick.

Trimmed from the N0300 harness; see README.md for the commands.
"""

import argparse
import fcntl
import hashlib
import itertools
import json
import os
import re
import shutil
import signal
import stat
import struct
import subprocess
import sys
import tempfile
import threading
import time
import zlib
from pathlib import Path

HERE = Path(__file__).resolve().parent
FIXTURES = HERE / "fixtures"
# (baseline, idle, cycles, rest) seconds. "chunk" fits one desktop-lock hold (~520 s).
PROFILES = {
    "short": (30, 240, 3, 60),
    "chunk": (30, 300, 6, 90),
    "full": (60, 600, 6, 1140),
}
LAYER = {"noctalia": "noctalia-bar-default", "nacre": "bead-dev"}
CLK_TCK = os.sysconf("SC_CLK_TCK")
PAGE = os.sysconf("SC_PAGE_SIZE")
NIRI_CONFIG = """input {
    keyboard {
        xkb {
            layout "us"
        }
    }
}
prefer-no-csd
hotkey-overlay {
    skip-at-startup
}
"""


def log(message):
    print(time.strftime("%H:%M:%S ") + message, flush=True)


def read(path):
    try:
        return Path(path).read_text()
    except (OSError, UnicodeDecodeError):
        return None


def read_int(path):
    text = read(path)
    try:
        return int(text.strip()) if text else None
    except ValueError:
        return None


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def wait_until(check, timeout, what):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = check()
        if value:
            return value
        time.sleep(0.1)
    raise RuntimeError("timeout waiting for " + what)


def static_png(path, width=1920, height=1080):
    """N0300's dark-blue 1920x1080 wallpaper; its SHA-256 matches N0300's only with stock zlib
    (zlib-ng on CachyOS compresses the same pixels differently)."""

    def chunk(kind, data):
        body = kind + data
        crc = struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)
        return struct.pack(">I", len(data)) + body + crc

    row = b"\x00" + bytes((22, 31, 43)) * width
    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    payload = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header)
    payload += chunk(b"IDAT", zlib.compress(row * height, 6)) + chunk(b"IEND", b"")
    path.write_bytes(payload)
    return sha256(path)


# ------------------------------------------------------------------ /proc sampling


def proc_table():
    """pid -> (ppid, start_ticks, utime+stime+cutime+cstime)."""
    table = {}
    for entry in os.scandir("/proc"):
        if not entry.name.isdigit():
            continue
        text = read(f"/proc/{entry.name}/stat")
        if not text:
            continue
        tail = text[text.rfind(")") + 2 :].split()
        try:
            table[int(entry.name)] = (
                int(tail[1]),
                int(tail[19]),
                int(tail[11]) + int(tail[12]) + int(tail[13]) + int(tail[14]),
            )
        except (ValueError, IndexError):
            continue
    return table


def descendants(root, table):
    found = {root}
    while True:
        more = {pid for pid, (ppid, _, _) in table.items() if ppid in found}
        if more <= found:
            return found
        found |= more


def start_ticks(pid):
    text = read(f"/proc/{pid}/stat")
    if not text:
        return None
    try:
        return int(text[text.rfind(")") + 2 :].split()[19])
    except (ValueError, IndexError):
        return None


def kill(pid, start, sig):
    """Signal pid only while it is still the process started at `start`."""
    if start is not None and start_ticks(pid) == start:
        try:
            os.kill(pid, sig)
        except (ProcessLookupError, PermissionError):
            pass


def threads(pid):
    """(tid, start, comm, cpu_ns, ctx) for every live thread of pid."""
    rows = []
    try:
        tids = os.listdir(f"/proc/{pid}/task")
    except OSError:
        return rows
    for tid in tids:
        base = f"/proc/{pid}/task/{tid}"
        sched = read(base + "/schedstat")
        status = read(base + "/status")
        start = start_ticks(f"{pid}/task/{tid}")
        if not sched or not status or start is None:
            continue
        ctx = 0
        for line in status.splitlines():
            if line.startswith(
                ("voluntary_ctxt_switches", "nonvoluntary_ctxt_switches")
            ):
                ctx += int(line.split()[1])
        comm = (read(base + "/comm") or "?").strip()
        rows.append((int(tid), start, comm, int(sched.split()[0]), ctx))
    return rows


def memory(pid):
    """(pss, unreclaimable = Pss_Anon + Pss_Shmem + SwapPss, swap_pss) in KiB."""
    text = read(f"/proc/{pid}/smaps_rollup")
    if not text:
        return (0, 0, 0)
    fields = {}
    for line in text.splitlines():
        if ":" in line:
            key, _, value = line.partition(":")
            fields[key] = int(value.split()[0]) if value.split() else 0
    swap = fields.get("SwapPss", 0)
    return (
        fields.get("Pss", 0),
        fields.get("Pss_Anon", 0) + fields.get("Pss_Shmem", 0) + swap,
        swap,
    )


UNITS = {"KiB": 1, "MiB": 1024, "GiB": 1048576}


def gpu(pid):
    """(gfx ns, resident GPU memory KiB) from amdgpu fdinfo, one entry per drm-client-id.

    GL buffers live outside PSS, so a GL shell's textures and swapchain only show here;
    buffers shared with the compositor are counted on both sides.
    """
    seen = {}
    try:
        fds = os.listdir(f"/proc/{pid}/fdinfo")
    except OSError:
        return 0, 0
    for fd in fds:
        text = read(f"/proc/{pid}/fdinfo/{fd}")
        if not text or "drm-client-id" not in text:
            continue
        fields = dict(
            line.split(":", 1) for line in text.splitlines() if line.startswith("drm-")
        )
        # Newer kernels report drm-resident-<region>; older amdgpu only drm-memory-<region>.
        prefix = (
            "drm-resident-"
            if any(k.startswith("drm-resident-") for k in fields)
            else "drm-memory-"
        )
        kib = 0
        for key, value in fields.items():
            parts = value.split()
            if key.startswith(prefix) and parts:
                kib += int(parts[0]) * UNITS.get(parts[1] if len(parts) > 1 else "", 0)
        ns = fields.get("drm-engine-gfx", "0").split()
        seen[fields["drm-client-id"].strip()] = (int(ns[0]) if ns else 0, kib)
    return sum(v[0] for v in seen.values()), sum(v[1] for v in seen.values())


def sensors(card):
    row = {
        "gpu_busy_pct": read_int(f"/sys/class/drm/{card}/device/gpu_busy_percent"),
        "vram_used_bytes": read_int(f"/sys/class/drm/{card}/device/mem_info_vram_used"),
        "ppt_uw": None,
        "tctl_mc": None,
        "edge_mc": None,
    }
    for hwmon in Path("/sys/class/hwmon").glob("hwmon*"):
        for label in hwmon.glob("power*_label"):
            if (read(label) or "").strip() == "PPT":
                base = label.name[: -len("_label")]
                row["ppt_uw"] = read_int(hwmon / f"{base}_input") or read_int(
                    hwmon / f"{base}_average"
                )
        for label in hwmon.glob("temp*_label"):
            name = (read(label) or "").strip().lower()
            key = "tctl_mc" if name == "tctl" else "edge_mc" if name == "edge" else None
            if key and row[key] is None:
                row[key] = read_int(hwmon / label.name.replace("_label", "_input"))
    return row


def meminfo():
    values = {}
    for line in (read("/proc/meminfo") or "").splitlines():
        key, _, value = line.partition(":")
        values[key] = int(value.split()[0])
    return values


# libwayland-client's WAYLAND_DEBUG line, stamped [HH:MM:SS.us] (UTC, 1.24+) or [ms.us]
# (ms of the epoch mod 1e6, older): ... -> wl_surface#ID.commit()
COMMIT = re.compile(
    r"\[\s*(?:(\d+):(\d+):(\d+)\.(\d+)|(\d+)\.(\d+))\][^\n]*-> wl_surface[@#](\d+)\.commit\(\)"
)


def commits(text):
    """[(ms, wrap period ms, surface id)] for each commit line."""
    rows = []
    for h, m, s, us, ms, frac, surface in COMMIT.findall(text):
        if h:
            rows.append(
                (
                    (int(h) * 60 + int(m)) * 60e3 + int(s) * 1e3 + int(us) / 1e3,
                    864e5,
                    surface,
                )
            )
        else:
            rows.append((int(ms) + int(frac) / 1e3, 1e6, surface))
    return rows


def frame_stats(text, opened):
    """niri's commits to labwc after an open call: one per composited frame.

    first_ms is open call to the first composited frame; the burst is that frame plus
    every following one closer than 100 ms; late counts gaps over 1.5 refreshes (25 ms).
    """
    hits = commits(text)
    if not hits:
        return None
    ids = [s for _, _, s in hits]
    surface = max(set(ids), key=ids.count)
    # Both stamps wrap (day, 1000 s); anything not 0..5 s after the call is not ours.
    times = [(ms - opened * 1e3) % period for ms, period, s in hits if s == surface]
    burst = [ms for ms in times if ms < 5000][:1]
    for ms in times[times.index(burst[0]) + 1 :] if burst else []:
        if ms - burst[-1] >= 100:
            break
        burst.append(ms)
    if not burst:
        return None
    gaps = [b - a for a, b in itertools.pairwise(burst)]
    return {
        "first_ms": round(burst[0], 1),
        "frames": len(burst),
        "span_ms": round(burst[-1] - burst[0], 1),
        "max_gap_ms": round(max(gaps, default=0), 1),
        "late": sum(g > 25 for g in gaps),
    }


assert frame_stats(
    "[999999.000]  -> wl_surface#7.commit()\n[     10.500] {q} -> wl_surface#7.commit()\n"
    "[     45.000]  -> wl_surface@7.commit()\n[    200.000]  -> wl_surface#7.commit()\n",
    999990.0 / 1e3,
) == {"first_ms": 9.0, "frames": 3, "span_ms": 46.0, "max_gap_ms": 34.5, "late": 1}
assert frame_stats(
    "[23:59:59.999000] -> wl_surface#3.commit()\n[00:00:00.016000] -> wl_surface#3.commit()\n",
    86399.990,
) == {"first_ms": 9.0, "frames": 2, "span_ms": 17.0, "max_gap_ms": 17.0, "late": 0}


def note(path, text):
    with path.open("a") as handle:
        handle.write(f"{time.strftime('%H:%M:%S')} {text}\n")


def gate_problem():
    """Desktop TEST host rules: swap < 64 MiB, MemAvailable > swap + 8 GiB, a quiet host."""
    info = meminfo()
    used = info["SwapTotal"] - info["SwapFree"]
    if used >= 64 * 1024 or info["MemAvailable"] <= used + 8 * 1024 * 1024:
        return f"swap used {used} KiB, MemAvailable {info['MemAvailable']} KiB"
    load = float((read("/proc/loadavg") or "0").split()[0])
    pressure = (read("/proc/pressure/cpu") or "avg10=0").split("avg10=")[1].split()[0]
    if load >= 4 or float(pressure) >= 5:
        return f"busy host: load1 {load}, cpu pressure avg10 {pressure}%"
    return None


def desktop_gate(wait_s=0, log=None):
    """Wait up to wait_s (polling each minute) for the gate, then fail the arm."""
    deadline = time.monotonic() + wait_s
    while problem := gate_problem():
        if time.monotonic() >= deadline:
            raise SystemExit("desktop gate: " + problem)
        if log:
            log(f"gate waiting: {problem}")
        time.sleep(60)


def swapin_pages():
    for line in (read("/proc/vmstat") or "").splitlines():
        if line.startswith("pswpin "):
            return int(line.split()[1])
    return 0


class Tree:
    """One measured process tree; thread counters are retained after threads exit."""

    def __init__(self, name, with_gpu):
        self.name, self.with_gpu = name, with_gpu
        self.roots = []
        self.threads = {}

    def sample(self, table):
        pids = set()
        for root in self.roots:
            if root in table:
                pids |= descendants(root, table)
        pss = unrec = swap = ticks = gpu_time = gpu_kib = 0
        for pid in pids:
            ticks += table[pid][2]
            p, u, s = memory(pid)
            pss, unrec, swap = pss + p, unrec + u, swap + s
            for tid, start, comm, cpu, ctx in threads(pid):
                self.threads[f"{pid}:{tid}:{start}"] = (comm, cpu, ctx)
            if self.with_gpu:
                ns, kib = gpu(pid)
                gpu_time, gpu_kib = gpu_time + ns, gpu_kib + kib
        return {
            "pids": sorted(pids),
            "cpu_ns": sum(v[1] for v in self.threads.values()),
            "cpu_ticks": ticks,
            "ctx": sum(v[2] for v in self.threads.values()),
            "pss_kib": pss,
            "unreclaimable_kib": unrec,
            "swap_pss_kib": swap,
            "gpu_ns": gpu_time,
            "gpu_mem_kib": gpu_kib,
        }


# ------------------------------------------------------------------ desktop lock


class ChunkLock:
    """Hold the shared desktop lock in chunks under 590 s; contention aborts the arm.

    A "chunk" arm ends before the first renewal, so other jobs interleave between arms."""

    def __init__(self, path, out):
        self.stream = open(path, "a")
        self.log = out / "lock.log"
        self.contended = False
        self.acquired = None
        self.stop = threading.Event()
        started = time.monotonic()
        fcntl.flock(self.stream, fcntl.LOCK_EX)
        self.acquired = time.monotonic()
        self.note(f"acquired waited_s={self.acquired - started:.1f}")
        threading.Thread(target=self.renew, daemon=True).start()

    def note(self, text):
        note(self.log, text)

    def renew(self):
        while not self.stop.wait(560):
            held = time.monotonic() - self.acquired
            fcntl.flock(self.stream, fcntl.LOCK_UN)
            # A queued job takes the lock now instead of racing our relock.
            time.sleep(2)
            try:
                fcntl.flock(self.stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                self.contended = True
                self.note(f"released held_s={held:.1f}; contended, arm aborts")
                return
            self.acquired = time.monotonic()
            self.note(f"renewed held_s={held:.1f}")

    def release(self):
        self.stop.set()
        if not self.contended:
            self.note(f"released held_s={time.monotonic() - self.acquired:.1f}")
            fcntl.flock(self.stream, fcntl.LOCK_UN)
        self.stream.close()


# ------------------------------------------------------------------ the arm


class Arm:
    def __init__(self, args):
        self.args = args
        self.arm = args.arm
        self.stage = args.stage
        self.out = args.out
        self.out.mkdir(parents=True, exist_ok=True)
        # Lock and gate first: a refused arm leaves nothing behind.
        self.lock = None
        if args.lock:
            # Wait for a quiet host without holding the lock (the owner may be using it).
            desktop_gate(12 * 3600, lambda text: note(self.out / "lock.log", text))
            self.lock = ChunkLock(args.lock, self.out)
            desktop_gate()
        self.private = args.out / "private"
        if self.private.exists():
            shutil.rmtree(self.private)
        self.logs = self.private / "logs"
        # Socket paths must stay under 108 bytes, as they do in /run/user/UID.
        self.runtime_root = Path(tempfile.mkdtemp(prefix="nacre-bench-"))
        self.runtime = self.runtime_root / "r"
        for name in ("home", "config", "data", "state/nacre", "cache", "bin", "logs"):
            (self.private / name).mkdir(parents=True, mode=0o700)
        self.runtime.mkdir(mode=0o700)
        self.procs = []
        self.owned = {}
        self.card = self.drm_card()
        self.env = self.base_env()
        self.niri = self.private / "bin/niri"
        self.niri.symlink_to(args.niri.resolve())
        self.noctalia = self.stage / "noctalia/bin/noctalia"
        self.helper = self.stage / "bin/nacre-helper"
        self.helper_env = None
        self.trees = {
            name: Tree(name, args.render_node is not None)
            for name in ("shell", "daemon", "niri", "labwc")
        }
        self.samples = []
        self.checkpoints = {}
        self.cycle_marks = []
        self.started = None

    def drm_card(self):
        if not self.args.render_node:
            return "card0"
        node = self.args.render_node.name
        device = Path(f"/sys/class/drm/{node}/device")
        for card in device.glob("drm/card*"):
            return card.name
        return "card0"

    def base_env(self):
        keep = ("LANG", "LC_ALL", "TERM", "USER", "LOGNAME", "SHELL")
        env = {key: value for key, value in os.environ.items() if key in keep}
        env.update(
            PATH=f"{self.private / 'bin'}:{self.stage / 'bin'}:/usr/bin:/bin",
            HOME=str(self.private / "home"),
            XDG_CONFIG_HOME=str(self.private / "config"),
            XDG_DATA_HOME=str(self.private / "data"),
            XDG_STATE_HOME=str(self.private / "state"),
            XDG_CACHE_HOME=str(self.private / "cache"),
            XDG_RUNTIME_DIR=str(self.runtime),
            XDG_DATA_DIRS="/usr/local/share:/usr/share",
            PULSE_SERVER="unix:" + str(self.private / "no-pulse"),
            GIO_USE_VFS="local",
            GVFS_DISABLE_FUSE="1",
            GTK_USE_PORTAL="0",
        )
        if self.args.render_node:
            slot = ""
            for line in (
                read(f"/sys/class/drm/{self.args.render_node.name}/device/uevent") or ""
            ).splitlines():
                if line.startswith("PCI_SLOT_NAME="):
                    slot = "pci-" + line.split("=", 1)[1].replace(":", "_").replace(
                        ".", "_"
                    )
            env.update(
                WLR_RENDER_DRM_DEVICE=str(self.args.render_node),
                DRI_PRIME=slot,
                __EGL_VENDOR_LIBRARY_FILENAMES="/usr/share/glvnd/egl_vendor.d/50_mesa.json",
            )
        else:
            env.update(
                WLR_RENDERER_ALLOW_SOFTWARE="1",
                LIBGL_ALWAYS_SOFTWARE="1",
                GALLIUM_DRIVER="llvmpipe",
            )
        return env

    # ---- processes

    def spawn(self, command, name, env=None, cwd=None):
        process = subprocess.Popen(
            command,
            env=env or self.env,
            cwd=cwd or self.private,
            stdout=(self.logs / f"{name}.log").open("w"),
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        self.procs.append((name, process))
        self.owned[process.pid] = start_ticks(process.pid)
        return process

    def run(self, command, timeout=15, check=True, env=None):
        return subprocess.run(
            [str(part) for part in command],
            env=env or self.env,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=check,
        )

    def remember(self, table):
        for pid in list(self.owned):
            if pid in table and table[pid][1] == self.owned[pid]:
                for child in descendants(pid, table):
                    self.owned.setdefault(child, table[child][1])

    def stop(self, process):
        """Terminate one owned root and its descendants by exact pid + start time."""
        table = proc_table()
        self.remember(table)
        targets = {
            pid: self.owned[pid]
            for pid in descendants(process.pid, table)
            if pid in self.owned and table.get(pid, (0, None))[1] == self.owned[pid]
        }
        for pid, start in targets.items():
            kill(pid, start, signal.SIGTERM)
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            process.poll()
            if not any(start_ticks(pid) == start for pid, start in targets.items()):
                break
            time.sleep(0.1)
        for pid, start in targets.items():
            kill(pid, start, signal.SIGKILL)
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            pass

    def shutdown(self):
        for _name, process in reversed(self.procs):
            if process.poll() is None:
                self.stop(process)
        for name, _process in self.procs:
            source = self.logs / f"{name}.log"
            if source.is_file():
                shutil.copyfile(source, self.out / f"{name}.log")
        shutil.rmtree(self.runtime_root, ignore_errors=True)
        if self.lock:
            self.lock.release()

    def check_alive(self):
        for name, process in self.procs:
            if (
                name in ("shell", "daemon", "niri", "labwc")
                and process.poll() is not None
            ):
                raise RuntimeError(
                    f"{name} exited with {process.returncode}; see {name}.log"
                )
        if self.lock and self.lock.contended:
            raise RuntimeError(
                "desktop lock contended at a chunk boundary; arm invalid"
            )
        if self.args.mem_rail_gib:
            available = meminfo().get("MemAvailable", 0)
            if available < self.args.mem_rail_gib * 1024 * 1024:
                raise RuntimeError(f"MemAvailable {available} KiB below the abort rail")

    # ---- fixture

    def start_bus(self):
        bus = subprocess.Popen(
            [
                "dbus-daemon",
                "--nofork",
                "--print-address=1",
                "--config-file",
                str(FIXTURES / "dbus-session.conf"),
            ],
            env=self.env,
            stdout=subprocess.PIPE,
            stderr=(self.logs / "dbus.log").open("w"),
            text=True,
            start_new_session=True,
        )
        self.procs.append(("dbus", bus))
        self.owned[bus.pid] = start_ticks(bus.pid)
        address = bus.stdout.readline().strip()
        bus.stdout.close()
        if not address:
            raise RuntimeError("private D-Bus did not print an address")
        self.env["DBUS_SESSION_BUS_ADDRESS"] = address
        self.env["DBUS_SYSTEM_BUS_ADDRESS"] = address

    def start_compositors(self):
        parent_runtime = self.runtime_root / "p"
        parent_runtime.mkdir(mode=0o700)
        config = self.private / "labwc"
        config.mkdir()
        (config / "autostart").write_text("")
        (config / "rc.xml").write_text(
            '<labwc_config><windowRules><windowRule identifier="*" serverDecoration="no">'
            '<action name="ResizeTo" width="1920" height="1080"/>'
            '<action name="MoveTo" x="0" y="0"/></windowRule></windowRules></labwc_config>'
        )
        parent_env = dict(
            self.env,
            XDG_RUNTIME_DIR=str(parent_runtime),
            WLR_BACKENDS="headless",
            WLR_HEADLESS_OUTPUTS="1",
            WLR_LIBINPUT_NO_DEVICES="1",
            WLR_RENDERER="gles2" if self.args.render_node else "pixman",
        )
        self.spawn(["labwc", "--config-dir", str(config)], "labwc", parent_env)
        socket = wait_until(
            lambda: next(
                (
                    f
                    for f in parent_runtime.glob("wayland-*")
                    if stat.S_ISSOCK(f.stat().st_mode)
                ),
                None,
            ),
            20,
            "labwc socket",
        )
        parent_env["WAYLAND_DISPLAY"] = str(socket)
        randr = self.stage / "bin/wlr-randr"
        outputs = json.loads(self.run([randr, "--json"], env=parent_env).stdout)
        self.run(
            [randr, "--output", outputs[0]["name"], "--custom-mode", "1920x1080@60Hz"],
            env=parent_env,
        )
        rules = self.stage / "nacre/config/niri/window-rules.kdl"
        text = NIRI_CONFIG + f"include {json.dumps(str(rules))}\n"
        text += (self.stage / "nacre/shell/niri-pearls.kdl").read_text()
        text += "".join(f'workspace "ws{i}"\n' for i in range(1, 5))
        niri_config = self.private / "niri.kdl"
        niri_config.write_text(text)
        self.env["NACRE_NIRI_CONFIG"] = str(niri_config)
        # WAYLAND_DEBUG=client logs only niri's own requests to labwc: the frame probe.
        self.spawn(
            [self.niri, "--config", str(niri_config)],
            "niri",
            dict(self.env, WAYLAND_DISPLAY=str(socket), WAYLAND_DEBUG="client"),
        )
        display = wait_until(
            lambda: next(
                (
                    f
                    for f in self.runtime.glob("wayland-*")
                    if stat.S_ISSOCK(f.stat().st_mode)
                ),
                None,
            ),
            20,
            "niri display",
        )
        niri_socket = wait_until(
            lambda: next(iter(self.runtime.glob("niri.*.sock")), None),
            20,
            "niri socket",
        )
        self.env.update(WAYLAND_DISPLAY=display.name, NIRI_SOCKET=str(niri_socket))

        def sized():
            # niri maps at 1280x800; labwc's ResizeTo rule lands a moment later.
            outputs = json.loads(self.niri_msg("-j", "outputs").stdout)
            output = next(iter(outputs.values()))
            mode = output["modes"][output["current_mode"]]
            return (
                (output, mode)
                if (mode["width"], mode["height"]) == (1920, 1080)
                else None
            )

        output, mode = wait_until(sized, 20, "a 1920x1080 nested output")
        if output["logical"]["scale"] != 1:
            raise RuntimeError(f"nested output is not scale 1: {output}")
        if abs(mode.get("refresh_rate", 0) - 60000) > 100:
            raise RuntimeError(f"nested output is not 60 Hz: {mode}")
        self.trees["niri"].roots = [self.procs[-1][1].pid]
        self.trees["labwc"].roots = [p.pid for n, p in self.procs if n == "labwc"]

    def niri_msg(self, *args):
        return self.run([self.niri, "msg", *args])

    def start_services(self):
        conf = self.private / "config/pipewire/pipewire.conf.d"
        conf.mkdir(parents=True)
        sys.path.insert(0, str(FIXTURES))
        import fake_system

        (conf / "10-fixture.conf").write_text(fake_system.pipewire_conf())
        self.env["PIPEWIRE_REMOTE"] = "pipewire-0"
        self.spawn(["pipewire"], "pipewire")
        wait_until(
            lambda: (
                self.run(["pw-metadata", "-n", "default"], check=False).returncode == 0
            ),
            20,
            "PipeWire",
        )
        for key, value in fake_system.default_metadata():
            self.run(
                ["pw-metadata", "-n", "default", "0", key, value, "Spa:String:JSON"]
            )
        for name in ("pactl", "wpctl", "parec"):
            wrapper = self.private / "bin" / name
            wrapper.write_text(
                f'#!/bin/sh\nFAKE_TOOL={name} exec {sys.executable} -I {FIXTURES / "fake_system.py"} "$@"\n'
            )
            wrapper.chmod(0o755)
        self.spawn([sys.executable, str(FIXTURES / "fake-power.py")], "fake-power")
        self.spawn([sys.executable, str(FIXTURES / "fake-bluez.py")], "fake-bluez")
        gdbus = ["gdbus", "call", "--session", "--dest"]
        wait_until(
            lambda: (
                self.run(
                    [
                        *gdbus,
                        "org.freedesktop.UPower",
                        "--object-path",
                        "/org/freedesktop/UPower",
                        "--method",
                        "org.freedesktop.UPower.GetDisplayDevice",
                    ],
                    check=False,
                ).returncode
                == 0
            ),
            20,
            "fake UPower",
        )
        wait_until(
            lambda: (
                self.run(
                    [
                        *gdbus,
                        "org.bluez",
                        "--object-path",
                        "/",
                        "--method",
                        "org.freedesktop.DBus.ObjectManager.GetManagedObjects",
                    ],
                    check=False,
                ).returncode
                == 0
            ),
            20,
            "fake BlueZ",
        )

    def install_fonts(self):
        manifest = json.loads(
            (self.stage / "nacre/nacre/shell-tools/font-assets.json").read_text()
        )
        expected = {
            item["name"]: item["sha256"]
            for group in manifest["groups"]
            if group.get("default")
            for item in group["files"]
        }
        target = self.private / "home/.local/share/fonts/nacre"
        target.mkdir(parents=True)
        for name, digest in expected.items():
            source = self.stage / "fonts" / name
            if sha256(source) != digest:
                raise RuntimeError(f"font asset hash mismatch: {source}")
            shutil.copyfile(source, target / name)
        (self.private / "data/fonts").symlink_to(target.parent)
        self.run(["fc-cache", "-f", str(target.parent)], timeout=120)
        matches = {}
        for family, name in (
            ("IBM Plex Sans", "IBMPlexSans.ttf"),
            ("Material Symbols Rounded", "MaterialSymbolsRounded.ttf"),
        ):
            line = self.run(["fc-match", "-f", "%{family}|%{file}", family]).stdout
            actual, _, path = line.partition("|")
            if family not in actual or sha256(path) != expected[name]:
                raise RuntimeError(
                    f"fontconfig did not select the pinned {family}: {line!r}"
                )
            matches[family] = path
        return matches

    # ---- shells

    def nacre_call(self, method, params, timeout=3):
        reply = self.run(
            [
                self.helper,
                "ctl",  # the nacrectl client; a bare "call" goes to the daemon as a helper tool
                "call",
                "--timeout",
                str(timeout * 1000),
                method,
                json.dumps(params),
            ],
            timeout=timeout + 2,
            check=False,
        )
        if reply.returncode:
            raise RuntimeError(
                f"daemon call failed: {method} {reply.stderr.strip()[:200]}"
            )
        return json.loads(reply.stdout) if reply.stdout.strip() else None

    def dispatch(self, target, method, *args):
        reply = self.nacre_call(
            "ui.dispatch", {"target": target, "method": method, "args": list(args)}
        )
        if isinstance(reply, dict) and reply.get("err"):
            raise RuntimeError(str(reply["err"])[:160])

    def start_daemon(self):
        process = self.spawn(
            [self.helper, "daemon", "--notifications=daemon", "--behaviours=daemon"],
            "daemon",
        )
        started = time.monotonic()
        last = []

        def ready():
            reply = self.run(
                [
                    self.helper,
                    "ctl",
                    "call",
                    "--timeout",
                    "3000",
                    "daemon.status",
                    "{}",
                ],
                check=False,
            )
            last[:] = [reply.stderr.strip()[-300:]]
            return reply.returncode == 0

        try:
            wait_until(ready, 30, "Nacre daemon")
        except RuntimeError as error:
            raise RuntimeError(f"{error}; last call: {last}") from None
        return process, time.monotonic() - started

    def prepare_nacre(self, wallpaper, palette):
        (self.runtime / "nacre").mkdir(mode=0o700)
        for part in ("shell", "shell/notifications", "shell/notifications/images"):
            (self.private / "state/nacre" / part).mkdir(mode=0o700, exist_ok=True)
        self.env.update(
            NACRE_DAEMON_SOCKET=str(self.runtime / "nacre/daemon.sock"),
            NACRE_COMPOSITOR="niri",
            NACRE_NIRI_HOVER_STREAM="0",
            NACRE_HELPER_TOOLS=str(self.stage / "nacre/nacre/shell-tools"),
            NACRE_HELPER_BINARY=str(self.helper),
            NACRE_HELPER=str(self.helper),
        )
        fonts = self.install_fonts()
        # The wallpaper is selected through the product's own feature with a
        # setup daemon, which is then stopped so the baseline excludes it.
        process, _ = self.start_daemon()
        self.run([self.helper, "wallpaper-media", "select", str(wallpaper)], timeout=60)
        record_path = self.private / "state/nacre/presentation.json"
        record = json.loads(record_path.read_text())
        record["mode"], record["colours"] = palette["mode"], palette["colours"]
        record_path.write_text(json.dumps(record, sort_keys=True) + "\n")
        self.stop(process)
        self.procs = [(n, p) for n, p in self.procs if p is not process]
        shell_root = self.private / "bead"
        shutil.copytree(self.stage / "nacre/shell", shell_root / "shell")
        (shell_root / "palette.json").write_text(
            json.dumps(
                {
                    "formatVersion": 1,
                    "mode": palette["mode"],
                    "colours": palette["colours"],
                }
            )
        )
        return {"fonts": fonts}

    def launch_nacre(self):
        daemon, daemon_ready = self.start_daemon()
        self.trees["daemon"].roots = [daemon.pid]
        root = self.private / "bead"
        command = [
            self.stage / "bin/nacre-shell",
            "--bar",
            "--core-views",
            "--ui-provider",
            "--notices",
            "--daemon-socket",
            self.env["NACRE_DAEMON_SOCKET"],
            "--command-socket",
            self.runtime / "bead-command.sock",
            "--palette",
            root / "palette.json",
        ]
        launch = time.monotonic()
        shell = self.spawn([str(part) for part in command], "shell", cwd=root)
        self.wait_layer(LAYER["nacre"])
        return shell, time.monotonic() - launch, daemon_ready

    def prepare_noctalia(self, wallpaper, palette):
        source = palette["colours"]
        mapping = {
            "mPrimary": "primary",
            "mOnPrimary": "onPrimary",
            "mSecondary": "secondary",
            "mOnSecondary": "onSecondary",
            "mTertiary": "tertiary",
            "mOnTertiary": "onTertiary",
            "mError": "error",
            "mOnError": "onError",
            "mSurface": "surface",
            "mOnSurface": "onSurface",
            "mSurfaceVariant": "surfaceVariant",
            "mOnSurfaceVariant": "onSurfaceVariant",
            "mOutline": "outline",
            "mShadow": "shadow",
            "mHover": "surfaceContainerHigh",
            "mOnHover": "onSurface",
        }
        dark = {
            key: "#" + str(source[token]).lstrip("#")
            for key, token in mapping.items()
            if token in source
        }
        ansi = ("black", "red", "green", "yellow", "blue", "magenta", "cyan", "white")
        dark["terminal"] = {
            "background": "#" + source["surface"],
            "foreground": "#" + source["onSurface"],
            "cursor": "#" + source["primary"],
            "cursorText": "#" + source["onPrimary"],
            "selectionBg": "#" + source["primary"],
            "selectionFg": "#" + source["onPrimary"],
            "normal": {key: "#" + source[f"term{i}"] for i, key in enumerate(ansi)},
            "bright": {key: "#" + source[f"term{i + 8}"] for i, key in enumerate(ansi)},
        }
        config = self.private / "config/noctalia"
        (config / "palettes").mkdir(parents=True)
        (config / "palettes/nacre-dark.json").write_text(
            json.dumps({"version": 2, "dark": dark}, indent=2) + "\n"
        )
        (config / "config.toml").write_text(
            '[theme]\nmode = "dark"\nsource = "custom"\ncustom_palette = "nacre-dark"\n\n'
            f"[wallpaper.default]\npath = {json.dumps(str(wallpaper))}\n"
        )
        self.env["NOCTALIA_ASSETS_DIR"] = str(
            self.stage / "noctalia/share/noctalia/assets"
        )
        sans = self.run(["fc-match", "-f", "%{family}|%{file}", "sans-serif"]).stdout
        return {"fonts": {"sans-serif": sans}}

    def launch_noctalia(self, wallpaper):
        launch = time.monotonic()
        shell = self.spawn([str(self.noctalia)], "shell")
        self.wait_layer(LAYER["noctalia"])
        readiness = time.monotonic() - launch
        wait_until(
            lambda: (
                Path(
                    self.run(
                        [self.noctalia, "msg", "wallpaper-get"], check=False
                    ).stdout.strip()
                )
                == wallpaper
            ),
            30,
            "Noctalia wallpaper",
        )
        scheme = self.run([self.noctalia, "msg", "color-scheme-get"]).stdout.strip()
        if scheme != "custom nacre-dark":
            raise RuntimeError(f"Noctalia palette selection: {scheme!r}")
        return shell, readiness, None

    def layers(self):
        """Namespaces of the mapped layer surfaces, one entry per surface."""
        reply = self.run([self.niri, "msg", "-j", "layers"], check=False)
        return [layer["namespace"] for layer in json.loads(reply.stdout or "[]")]

    def wait_layer(self, namespace):
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline:
            if namespace in self.layers():
                return
            time.sleep(0.005)
        raise RuntimeError(f"no {namespace} layer surface within 60 s")

    # ---- workload

    def open_panel(self, panel):
        if self.arm == "noctalia":
            self.run(
                [
                    self.noctalia,
                    "msg",
                    "panel-toggle",
                    "launcher" if panel == "launcher" else "control-center",
                ]
            )
        elif panel == "launcher":
            self.dispatch("nacre", "launcher", "")
        else:
            self.dispatch("drawers", "toggle", "dashboard")
            self.dispatch("nacre", "tab", "0")

    def close_panels(self):
        if self.arm == "noctalia":
            for panel in ("launcher", "control-center"):
                self.run([self.noctalia, "msg", "panel-close", panel])
        else:
            self.dispatch("nacre", "close")

    def focus_workspace(self, index):
        self.niri_msg("action", "focus-workspace", index)
        deadline = time.monotonic() + 4
        while time.monotonic() < deadline:
            rows = json.loads(self.niri_msg("-j", "workspaces").stdout)
            active = next((r for r in rows if r.get("is_focused")), None)
            if active and str(active.get("idx")) == index:
                return
            time.sleep(0.05)
        raise RuntimeError(f"workspace {index} did not receive focus")

    def notify(self, summary):
        self.run(
            [
                "gdbus",
                "call",
                "--session",
                "--dest",
                "org.freedesktop.Notifications",
                "--object-path",
                "/org/freedesktop/Notifications",
                "--method",
                "org.freedesktop.Notifications.Notify",
                "bench",
                "0",
                "",
                summary,
                "Private benchmark notification",
                "[]",
                "{}",
                "2500",
            ]
        )

    def interaction(self, cycles):
        """N0300's 10 s cycle: launcher, dashboard, 4 workspace switches, 4 notifications."""
        for cycle in range(cycles):
            self.cycle_marks.append(time.monotonic())
            start = time.monotonic()
            self.open_panel("launcher")
            time.sleep(0.8)
            self.close_panels()
            self.open_panel("dashboard")
            time.sleep(0.8)
            self.close_panels()
            for index in ("2", "1", "3", "1"):
                self.focus_workspace(index)
                time.sleep(0.3)
            for index in range(4):
                self.notify(f"bench {cycle + 1}.{index + 1}")
                time.sleep(0.2)
            time.sleep(max(0, start + 10 - time.monotonic()))

    def open_frames(self, panel, repeats=5):
        """Frame probe per open: the IPC call (client start included) to niri's frames."""
        log_path = self.logs / "niri.log"
        results = []
        for _ in range(repeats):
            offset = log_path.stat().st_size
            opened = time.time()  # libwayland stamps CLOCK_REALTIME
            self.open_panel(panel)
            time.sleep(1.5)
            with log_path.open(errors="replace") as handle:
                handle.seek(offset)
                results.append(frame_stats(handle.read(), opened))
            self.close_panels()
            time.sleep(1.5)
        return results

    def renderer_evidence(self):
        words = ("egl", "gles", "glcore", "swrast", "llvmpipe", "nvidia")
        evidence = {}
        for name in ("shell", "niri", "labwc"):
            for pid in self.trees[name].roots:
                maps = read(f"/proc/{pid}/maps") or ""
                libs = sorted(
                    {
                        line.split()[-1].rsplit("/", 1)[-1]
                        for line in maps.splitlines()
                        if any(w in line.lower() for w in words)
                    }
                )
                fds = []
                for fd in Path(f"/proc/{pid}/fd").glob("*"):
                    try:
                        target = os.readlink(fd)
                    except OSError:
                        continue
                    if target.startswith("/dev/dri/"):
                        fds.append(target)
                evidence[name] = {"gl_libraries": libs, "drm_fds": sorted(set(fds))}
        return evidence

    # ---- measurement

    def sample_all(self, phase):
        table = proc_table()
        self.remember(table)
        row = {
            "t": round(time.monotonic() - self.started, 2),
            "phase": phase,
            "trees": {},
        }
        for name, tree in self.trees.items():
            if tree.roots:
                row["trees"][name] = tree.sample(table)
        if self.args.render_node:  # host sensors mean nothing on a shared build host
            row.update(sensors(self.card))
        row["memavailable_kib"] = meminfo().get("MemAvailable")
        self.samples.append(row)
        for name, data in row["trees"].items():
            if data["swap_pss_kib"]:
                raise RuntimeError(f"{name} tree has nonzero SwapPss; run invalid")
        return row

    def checkpoint(self, name, row):
        self.checkpoints[name] = {
            "t": row["t"],
            "trees": row["trees"],
            "threads": {tree: dict(self.trees[tree].threads) for tree in row["trees"]},
        }

    def measure(self):
        args = self.args
        baseline_s, idle_s, cycles, rest_s = args.durations
        wallpaper = self.private / "wallpaper.png"
        wallpaper_sha = static_png(wallpaper)
        palette = json.loads((self.stage / "nacre/shell/palette.json").read_text())
        if palette.get("mode") != "dark" or "term15" not in palette.get("colours", {}):
            raise RuntimeError(
                "shell/palette.json is not the dark palette with terminal roles"
            )
        self.start_bus()
        self.start_compositors()
        self.start_services()
        prepared = (
            self.prepare_nacre(wallpaper, palette)
            if self.arm == "nacre"
            else self.prepare_noctalia(wallpaper, palette)
        )
        noise = {"start": {"loadavg": read("/proc/loadavg"), "pswpin": swapin_pages()}}
        # Baseline: compositors and fixture only; neither measured shell nor daemon.
        self.started = time.monotonic()
        while time.monotonic() - self.started < baseline_s:
            self.check_alive()
            self.sample_all("baseline")
            time.sleep(max(0, 1 - (time.monotonic() - self.started) % 1))
        shell, readiness, daemon_ready = (
            self.launch_nacre()
            if self.arm == "nacre"
            else self.launch_noctalia(wallpaper)
        )
        self.trees["shell"].roots = [shell.pid]
        launched = time.monotonic()
        idle_end = idle_s
        worker = None
        errors = []
        cycles_seen = 0
        phase = "idle"
        end = idle_s + 10 * cycles + rest_s
        next_beat = 60
        while True:
            now = time.monotonic() - launched
            self.check_alive()
            row = self.sample_all(phase)
            if "startup" not in self.checkpoints:
                self.checkpoint("startup", row)
            if now >= 30 and "idle-start" not in self.checkpoints:
                self.checkpoint("idle-start", row)
            if now >= idle_end and worker is None:
                self.checkpoint("pre-interaction", row)
                phase = "interaction"
                if args.perf:
                    perf = self.start_perf(row)

                def work():
                    try:
                        self.interaction(cycles)
                    except (
                        RuntimeError,
                        subprocess.SubprocessError,
                        json.JSONDecodeError,
                    ) as error:
                        errors.append(f"{type(error).__name__}: {error}")

                worker = threading.Thread(target=work, daemon=True)
                worker.start()
            if len(self.cycle_marks) > cycles_seen:
                cycles_seen = len(self.cycle_marks)
                if cycles_seen == 2:
                    self.checkpoint("cycle2", row)
            if worker is not None and phase == "interaction" and not worker.is_alive():
                worker.join()
                if errors:
                    raise RuntimeError("interaction failed: " + errors[0])
                self.checkpoint("post-interaction", row)
                phase = "rest"
                if args.perf:
                    self.stop_perf(perf)
            if now >= next_beat:
                shell_data = row["trees"].get("shell", {})
                log(
                    f"{self.arm} {phase} t={now:.0f}s shell_pss={shell_data.get('pss_kib', 0) // 1024}MiB"
                )
                next_beat += 60
            if now >= end and phase == "rest":
                self.checkpoint("end", row)
                break
            time.sleep(max(0.05, 1 - ((time.monotonic() - launched) % 1)))
        owner = self.run(
            [
                "gdbus",
                "call",
                "--session",
                "--dest",
                "org.freedesktop.DBus",
                "--object-path",
                "/org/freedesktop/DBus",
                "--method",
                "org.freedesktop.DBus.GetNameOwner",
                "org.freedesktop.Notifications",
            ],
            check=False,
        )
        noise["end"] = {"loadavg": read("/proc/loadavg"), "pswpin": swapin_pages()}
        frames = {panel: self.open_frames(panel) for panel in ("launcher", "dashboard")}
        self.screenshots()
        shell_log = read(self.logs / "shell.log") or ""
        return {
            "arm": self.arm,
            "profile": list(args.durations),
            "render_node": str(args.render_node) if args.render_node else None,
            "wallpaper_sha256": wallpaper_sha,
            "readiness_s": round(readiness, 3),
            "daemon_ready_s": None if daemon_ready is None else round(daemon_ready, 3),
            "open_frames": frames,
            "notification_owner": owner.stdout.strip() or owner.stderr.strip(),
            "renderer": self.renderer_evidence(),
            "wallpaper_video_helper": "disabled"
            if "WALLPAPER_VIDEO_DISABLED" in shell_log
            else "present",
            "ui_errors": shell_log.count("UI_ERROR"),
            "host_swapin_mib": round(
                (noise["end"]["pswpin"] - noise["start"]["pswpin"]) * PAGE / 1048576, 3
            ),
            "noise": noise,
            "prepared": prepared,
            "pins": json.loads(read(self.stage / "pins.json") or "{}"),
            "binaries": {
                name: sha256(path)
                for name, path in (
                    ("noctalia", self.noctalia),
                    ("nacre-shell", self.stage / "bin/nacre-shell"),
                    ("nacre-helper", self.helper),
                    ("niri", args.niri),
                )
                if path.is_file()
            },
        }

    def start_perf(self, row):
        """Diagnostic only: flat CPU profile of the shell and daemon over the interaction."""
        pids = [
            p
            for name in ("shell", "daemon")
            for p in row["trees"].get(name, {}).get("pids", [])
        ]
        command = ["perf", "record", "-q", "-F", "499", "-o", self.out / "perf.data"]
        return self.spawn(
            [str(c) for c in command + ["-p", ",".join(map(str, pids))]], "perf"
        )

    def stop_perf(self, process):
        os.kill(process.pid, signal.SIGINT)  # perf writes its data on SIGINT
        process.wait(timeout=60)
        self.procs = [(n, p) for n, p in self.procs if p is not process]
        report = self.run(
            [
                "perf",
                "report",
                "-i",
                self.out / "perf.data",
                "--stdio",
                "--no-children",
                "--sort",
                "comm,dso,sym",
                "--percent-limit",
                "0.5",
            ],
            timeout=300,
            check=False,
        )
        (self.out / "perf.txt").write_text(report.stdout + report.stderr[-2000:])

    def screenshots(self):
        self.close_panels()
        time.sleep(1)
        self.run(["grim", str(self.out / "idle.png")], timeout=30)
        for panel in ("launcher", "dashboard"):
            self.open_panel(panel)
            time.sleep(1.5)
            self.run(["grim", str(self.out / f"{panel}.png")], timeout=30)
            self.close_panels()
            time.sleep(1)


# ------------------------------------------------------------------ results


def window(a, b, name):
    seconds = b["t"] - a["t"]
    x, y = a["trees"].get(name), b["trees"].get(name)
    if not x or not y or seconds <= 0:
        return None
    return {
        "seconds": round(seconds, 1),
        "cpu_pct": round((y["cpu_ns"] - x["cpu_ns"]) / 1e7 / seconds, 4),
        "cpu_pct_ticks": round(
            (y["cpu_ticks"] - x["cpu_ticks"]) * 100 / CLK_TCK / seconds, 4
        ),
        "ctx_per_s": round((y["ctx"] - x["ctx"]) / seconds, 2),
        "gpu_pct": round((y["gpu_ns"] - x["gpu_ns"]) / 1e7 / seconds, 4),
    }


def top_threads(a, b, limit=12):
    """CPU and wakeups per (tree, process, thread name), summed over same-named threads."""
    groups = {}
    for tree, table in b["threads"].items():
        before = a["threads"].get(tree, {})
        process = {
            key.split(":")[0]: comm
            for key, (comm, _, _) in table.items()
            if key.split(":")[0] == key.split(":")[1]
        }
        for key, (comm, cpu, ctx) in table.items():
            old = before.get(key, (comm, 0, 0))
            name = (tree, process.get(key.split(":")[0], "?"), comm)
            row = groups.setdefault(name, [0, 0, 0])
            row[0] += cpu - old[1]
            row[1] += ctx - old[2]
            row[2] += 1
    seconds = b["t"] - a["t"]
    rows = [
        {
            "tree": tree,
            "process": process,
            "thread": comm,
            "threads": count,
            "cpu_ms": round(cpu / 1e6, 1),
            "ctx_per_s": round(ctx / seconds, 2),
        }
        for (tree, process, comm), (cpu, ctx, count) in groups.items()
    ]
    return {
        "by_cpu": sorted(rows, key=lambda r: -r["cpu_ms"])[:limit],
        "by_wakeups": sorted(rows, key=lambda r: -r["ctx_per_s"])[:limit],
    }


def power(samples, lo, hi, baseline):
    rows = [s for s in samples if lo <= s["t"] < hi and s["phase"] != "baseline"]
    out = {}
    for key, scale in (
        ("ppt_uw", 1e6),
        ("gpu_busy_pct", 1),
        ("vram_used_bytes", 1048576),
    ):
        values = [s[key] for s in rows if s.get(key) is not None]
        base = [s[key] for s in baseline if s.get(key) is not None]
        if values and base:
            out[key] = {
                "mean": round(sum(values) / len(values) / scale, 3),
                "above_baseline": round(
                    (sum(values) / len(values) - sum(base) / len(base)) / scale, 3
                ),
                "n": len(values),
            }
    return out


def results(arm, meta):
    c = arm.checkpoints
    pairs = {
        "idle": ("idle-start", "pre-interaction"),
        "interaction": ("pre-interaction", "post-interaction"),
        "cycle1": ("pre-interaction", "cycle2"),
        "cycles_rest": ("cycle2", "post-interaction"),
        "rest": ("post-interaction", "end"),
    }
    windows = {}
    for label, (first, last) in pairs.items():
        if first in c and last in c:
            windows[label] = {
                name: window(c[first], c[last], name) for name in arm.trees
            }
            shell, daemon = windows[label]["shell"], windows[label]["daemon"]
            if shell:
                windows[label]["combined"] = {
                    k: round(shell[k] + (daemon[k] if daemon else 0), 4)
                    for k in ("cpu_pct", "cpu_pct_ticks", "ctx_per_s", "gpu_pct")
                }
            windows[label]["threads"] = top_threads(c[first], c[last])
    mem = {}
    for point in ("startup", "pre-interaction", "post-interaction", "end"):
        if point in c:
            trees = c[point]["trees"]
            mem[point] = {
                name: {
                    "pss_mib": round(trees[name]["pss_kib"] / 1024, 2),
                    "unreclaimable_mib": round(
                        trees[name]["unreclaimable_kib"] / 1024, 2
                    ),
                    "gpu_mib": round(trees[name]["gpu_mem_kib"] / 1024, 2),
                }
                for name in trees
            }
            mem[point]["combined"] = {
                k: round(
                    sum(mem[point][n][k] for n in ("shell", "daemon") if n in trees), 2
                )
                for k in ("pss_mib", "unreclaimable_mib", "gpu_mib")
            }
    baseline = [s for s in arm.samples if s["phase"] == "baseline"][-30:]
    meta.update(
        windows=windows,
        memory=mem,
        memory_growth_mib=round(
            mem["end"]["combined"]["pss_mib"]
            - mem["pre-interaction"]["combined"]["pss_mib"],
            2,
        )
        if "end" in mem
        else None,
        power={
            label: power(arm.samples, c[first]["t"], c[last]["t"], baseline)
            for label, (first, last) in pairs.items()
            if first in c and last in c
        },
        baseline_ppt_w=round(
            sum(s["ppt_uw"] for s in baseline if s.get("ppt_uw")) / 1e6 / len(baseline),
            3,
        )
        if baseline and baseline[0].get("ppt_uw")
        else None,
        checkpoints={k: {"t": v["t"], "trees": v["trees"]} for k, v in c.items()},
    )
    return meta


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--arm", choices=sorted(LAYER), required=True)
    parser.add_argument("--stage", type=Path, required=True, help="output of stage.sh")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--profile", choices=sorted(PROFILES), default="short")
    parser.add_argument(
        "--niri", type=Path, help="compositor binary; default stage/bin/stock-niri"
    )
    parser.add_argument(
        "--render-node",
        type=Path,
        help="AMD render node; software rendering without it",
    )
    parser.add_argument(
        "--lock", type=Path, help="shared desktop lock, held in <590 s chunks"
    )
    parser.add_argument(
        "--perf",
        action="store_true",
        help="diagnostic: perf record the shell and daemon during the interaction",
    )
    parser.add_argument(
        "--mem-rail-gib",
        type=float,
        default=0,
        help="abort when MemAvailable drops below",
    )
    args = parser.parse_args()
    args.stage = args.stage.resolve()
    args.out = args.out.resolve()
    args.niri = (args.niri or args.stage / "bin/stock-niri").resolve()
    args.durations = PROFILES[args.profile]
    if os.getpriority(os.PRIO_PROCESS, 0) < 10:
        os.setpriority(os.PRIO_PROCESS, 0, 10)
    os.umask(0o077)
    arm = Arm(args)
    status = "FAIL"
    try:
        meta = arm.measure()
        (args.out / "samples.json").write_text(json.dumps(arm.samples) + "\n")
        (args.out / "result.json").write_text(
            json.dumps(results(arm, meta), indent=1) + "\n"
        )
        status = "PASS"
    finally:
        if arm.samples and not (args.out / "samples.json").exists():
            (args.out / "samples.json").write_text(json.dumps(arm.samples) + "\n")
        arm.shutdown()
        (args.out / "status.txt").write_text(status + "\n")
        log(f"{args.arm}: {status}")


if __name__ == "__main__":
    main()
