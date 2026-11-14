#!/usr/bin/env python3
"""Shell motion and idle bench: Nacre alone in the N0308 nested session (noctalia/bench.py)
at 1920x1080@120 Hz, one short capture per call so each desktop-lock hold stays short.

  idle   --idle-s N: N s with no panel open after a 30 s settle: shell and daemon CPU %,
         wakeups/s (context switches), RSS and PSS.
  motion --panels a,b: per panel, --repeats opens and closes, then one open held idle for
         --hold-s (CPU, wakeups, RSS). Frames come from Bead's UI_FRAME trace
         (BEAD_FRAME_TRACE=1) and PANEL_SLIDE markers in shell.log.
  --analyze OUT...: the per-panel table from captured runs (phases.txt + shell.log).
"""

import argparse
import itertools
import json
import re
import statistics
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "noctalia"))
import bench  # noqa: E402

PANELS = (
    "dashboard",
    "launcher",
    "control",
    "control-hover",
    "control-click",
    "chat",
    "osd",
    "toast",
)
AUTO_HIDE = ("osd", "toast")  # one phase covers show and hide


class Capture:
    def __init__(self, a):
        a.arm, a.profile, a.lock, a.perf, a.durations = (
            "nacre",
            "short",
            None,
            False,
            bench.PROFILES["short"],
        )
        a.refresh_hz = 120
        self.a, self.arm = a, bench.Arm(a)
        self.arm.env.update(BEAD_FRAME_TRACE="1", BEAD_APP_LOG_DIR=str(a.out))
        # The private session has no user manager: run apps (chat) directly, not in a scope.
        shim = self.arm.private / "bin/systemd-run"
        shim.write_text(
            '#!/bin/sh\nwhile [ $# -gt 0 ]; do case "$1" in --*) shift;; *) break;; esac; done\nexec "$@"\n'
        )
        shim.chmod(0o755)
        self.pointer = None
        self.phases = a.out / "phases.txt"
        self.pswpin0 = bench.swapin_pages()

    def guard(self):
        """Lead waiver (2026-11-14): the capture is void once host swap-in passes 8 MiB or the host gets busy."""
        self.arm.check_alive()
        swapin = (bench.swapin_pages() - self.pswpin0) * bench.PAGE
        if swapin > 8 << 20:
            raise RuntimeError(f"host swap-in {swapin >> 20} MiB > 8 MiB: capture void")
        load = float(bench.read("/proc/loadavg").split()[0])
        pressure = float(bench.read("/proc/pressure/cpu").split("avg10=")[1].split()[0])
        if (
            load >= 4 or pressure >= 5
        ):  # the owner's session (or another job) became active
            raise RuntimeError(
                f"busy host: load1 {load}, cpu pressure {pressure}%: capture void"
            )

    def phase(self, name):
        with self.phases.open("a") as handle:
            handle.write(f"{name} {time.monotonic_ns()} {time.time():.6f}\n")

    def pt(self, cmd, pause=0.03):
        self.pointer.stdin.write(cmd + "\n")
        self.pointer.stdin.flush()
        time.sleep(pause)

    def at(self, x, y):
        self.pt(f"move {int(x)} {int(y)} 1920 1080")

    def sample(self):
        table = bench.proc_table()
        row = {name: self.arm.trees[name].sample(table) for name in ("shell", "daemon")}
        for name in ("shell", "daemon"):
            row[name]["rss_kib"] = sum(
                int(line.split()[1])
                for pid in row[name]["pids"]
                for line in (bench.read(f"/proc/{pid}/status") or "").splitlines()
                if line.startswith("VmRSS:")
            )
        row["t"], row["pswpin"] = time.monotonic(), bench.swapin_pages()
        return row

    def window(self, name, seconds):
        """CPU % of one core, wakeups/s and peak RSS over `seconds` (sampled every 5 s)."""
        first = self.sample()
        rows = [first]
        end = first["t"] + seconds
        while time.monotonic() < end:
            self.guard()
            time.sleep(min(5, max(0, end - time.monotonic())))
            rows.append(self.sample())
        last = rows[-1]
        span = last["t"] - first["t"]
        out = {
            "window": name,
            "seconds": round(span, 1),
            "swapin_pages": last["pswpin"] - first["pswpin"],
        }
        for tree in ("shell", "daemon"):
            a, b = first[tree], last[tree]
            out[tree] = {
                "cpu_pct": round((b["cpu_ns"] - a["cpu_ns"]) / span / 1e7, 3),
                "wakeups_per_s": round((b["ctx"] - a["ctx"]) / span, 1),
                "rss_mib_peak": round(max(r[tree]["rss_kib"] for r in rows) / 1024, 1),
                "pss_mib_end": round(b["pss_kib"] / 1024, 1),
                "swap_pss_kib_end": b["swap_pss_kib"],
                "pids": len(b["pids"]),
            }
        bench.log(f"{name}: {json.dumps(out)}")
        with (self.a.out / "windows.jsonl").open("a") as handle:
            handle.write(json.dumps(out) + "\n")

    def opener(self, panel):
        d, a = self.arm.dispatch, self.a
        return {
            "dashboard": (
                lambda: d("drawers", "toggle", "dashboard"),
                lambda: d("nacre", "close"),
            ),
            "launcher": (
                lambda: d("nacre", "launcher", ""),
                lambda: d("nacre", "close"),
            ),
            "control": (
                lambda: d("drawers", "toggle", "right"),
                lambda: d("nacre", "close"),
            ),
            # The docked control panel opens from the right-edge grabber after a hover dwell.
            "control-hover": (
                lambda: (self.at(1917, 540), time.sleep(0.9)),
                lambda: (self.at(960, 600), d("nacre", "close")),
            ),
            # A bar status icon: hover shows its peek ("open"), a click grows it ("grow").
            "control-click": (
                lambda: (
                    self.at(a.icon_x, a.bar_y),
                    self.at(a.icon_x + 2, a.bar_y + 1),
                    time.sleep(0.6),
                    self.phase(f"control-click grow {self.i}"),
                    self.pt("click"),
                ),
                lambda: (self.pt("click"), time.sleep(0.3), self.at(960, 600)),
            ),
            "chat": (lambda: d("nacre", "left"), lambda: d("nacre", "left")),
            # The OSD shows daemon osd events only: a volume change through the daemon.
            "osd": (
                lambda: self.arm.nacre_call(
                    "audio.set_volume",
                    {"target": "sink", "value": round(0.3 + time.time() % 0.4, 2)},
                ),
                None,
            ),
            "toast": (lambda: self.arm.notify("bench toast"), None),
        }[panel]

    def run(self):
        arm, a = self.arm, self.a
        try:
            wallpaper = arm.private / "wallpaper.png"
            bench.static_png(wallpaper)
            arm.start_bus()
            arm.start_compositors()
            arm.start_services()
            arm.prepare_nacre(
                wallpaper,
                json.loads((arm.stage / "nacre/shell/palette.json").read_text()),
            )
            shell, readiness, _ = arm.launch_nacre()
            arm.trees["shell"].roots = [shell.pid]
            arm.trees["daemon"].roots = [p.pid for n, p in arm.procs if n == "daemon"]
            bench.log(f"shell mapped in {readiness:.2f} s")
            self.pointer = subprocess.Popen(
                [str(a.pointer)],
                stdin=subprocess.PIPE,
                text=True,
                stdout=(a.out / "pointer.log").open("w"),
                stderr=subprocess.STDOUT,
                env=dict(arm.env, BEAD_PRIVATE_INPUT_PROOF="1"),
                start_new_session=True,
            )
            arm.procs.append(("pointer", self.pointer))
            arm.owned[self.pointer.pid] = bench.start_ticks(self.pointer.pid)
            self.at(960, 600)
            time.sleep(a.settle_s)
            arm.run(["grim", str(a.out / "idle.png")], timeout=30)
            if a.idle_s:
                self.window("idle-no-panel", a.idle_s)
            for panel in filter(None, a.panels.split(",")):
                show, hide = self.opener(panel)
                for self.i in range(a.repeats):
                    self.guard()
                    self.phase(f"{panel} open {self.i}")
                    attempt(show)
                    time.sleep(4.0 if hide is None else 1.2)
                    if hide:
                        self.phase(f"{panel} close {self.i}")
                        attempt(hide)
                        time.sleep(1.2)
                if hide and a.hold_s:
                    self.i = "hold"
                    self.phase(f"{panel} hold")
                    attempt(show)
                    time.sleep(3)
                    arm.run(["grim", str(a.out / f"{panel}.png")], timeout=30)
                    self.window(f"open-idle {panel}", a.hold_s)
                    self.phase(f"{panel} unhold")
                    attempt(hide)
                    time.sleep(1.5)
            self.phase("end")
            self.pt("stop")
        finally:
            arm.shutdown()


def attempt(action):
    try:
        action()
    except RuntimeError as error:  # one refused route must not end the capture
        bench.log(f"action failed: {error}")


def kv(line):
    return dict(re.findall(r"(\w+)=(\S+)", line))


def pct(values, q):
    values = sorted(values)
    return values[min(len(values) - 1, round(q * (len(values) - 1)))] if values else 0.0


def analyze(out):
    """{(panel, dir): [{first_ms, gaps, max_gap, slide}] per repeat} from one capture."""
    phases = [
        (n.rsplit(" ", 2)[0], int(n.rsplit(" ", 2)[1]), float(n.rsplit(" ", 2)[2]))
        for n in (out / "phases.txt").read_text().splitlines()
    ]
    composited = bench.commits((out / "niri.log").read_text(errors="replace"))
    surface = statistics.mode([s for _, _, s in composited])
    composited = [
        ms for ms, _, s in composited if s == surface
    ]  # niri's frames to labwc, ms of the UTC day
    frames, slides, last_ns = [], [], 0
    for line in (out / "shell.log").read_text(errors="replace").splitlines():
        if line.startswith("UI_FRAME "):
            d = kv(line)
            last_ns = int(d["monotonic_ns"])
            frames.append((last_ns, d))
        elif line.startswith(
            "PANEL_SLIDE "
        ):  # unstamped: placed at the frame before it
            slides.append((last_ns, kv(line)))
    for log in out.glob(
        "app-*.log"
    ):  # on-demand app processes (chat), BEAD_APP_LOG_DIR
        frames += [
            (int(d["monotonic_ns"]), d)
            for d in map(kv, log.read_text(errors="replace").splitlines())
            if "gap_ms" in d
        ]
    frames.sort(key=lambda f: f[0])
    rows = {}
    for i, (name, start, wall) in enumerate(phases[:-1]):
        parts = name.split()
        if len(parts) != 3 or not parts[2].isdigit():
            continue
        end = phases[i + 1][1]
        moving = [d for ns, d in frames if start <= ns < end and d["motion"] != "-"]
        gaps = [float(d["gap_ms"]) for d in moving[1:] if float(d["gap_ms"]) > 0]
        rows.setdefault((parts[0], parts[1]), []).append(
            {
                "first_ms": round((int(moving[0]["monotonic_ns"]) - start) / 1e6, 1)
                if moving
                else None,
                "frames": len(moving),
                "gaps": gaps,
                "composited": composited_gaps(
                    composited, wall, phases[i + 1][2], parts[0] in AUTO_HIDE
                ),
                "slide_max_gap": max(
                    (float(s["max_gap_ms"]) for ns, s in slides if start <= ns < end),
                    default=None,
                ),
            }
        )
    return rows


def composited_gaps(times, wall, wall_end, auto_hide):
    """(first frame ms, gaps): niri's frames from the call to the end of the last burst of >= 3."""
    lo, hi = wall % 86400 * 1e3, wall_end % 86400 * 1e3
    hits = [ms for ms in times if lo <= ms < hi]
    last, run = None, 0
    for j in range(1, len(hits)):
        run = run + 1 if hits[j] - hits[j - 1] < 100 else 0
        if run >= 2:
            last = j
    if last is None:
        return None, []
    gaps = [b - a for a, b in itertools.pairwise(hits[: last + 1])]
    # Overlays (OSD, toast) are not in the UI_FRAME trace and their show and hide are two
    # bursts: keep gaps inside bursts only; their PANEL_SLIDE max_gap is the stall measure.
    return round(hits[0] - lo, 1), [g for g in gaps if not auto_hide or g < 100]


def f(v):
    return "-" if v is None else f"{v:.1f}"


def table(outs):
    merged = {}
    for out in outs:
        for key, reps in analyze(out).items():
            merged.setdefault(key, []).append(reps)
    lines = [
        "| panel | dir | warm+cold n | shell frames | shell p50 | p95 | max | cold max | cold first | warm first | "
        "niri p50 | p95 | max | cold max | cold first | warm first | PANEL_SLIDE max |",
        "| --- " * 17 + "|",
    ]
    for (panel, way), runs in sorted(
        merged.items(), key=lambda kv: (PANELS.index(kv[0][0]), kv[0][1] != "open")
    ):
        cold, warm = [r[0] for r in runs], [rep for r in runs for rep in r[1:]]
        cells = [
            panel,
            way,
            f"{len(warm)}+{len(cold)}",
            str(statistics.median([r["frames"] for r in warm] or [0])),
        ]
        for gaps_of, first_of in (
            (lambda r: r["gaps"], lambda r: r["first_ms"]),
            (lambda r: r["composited"][1], lambda r: r["composited"][0]),
        ):
            gaps = [g for r in warm for g in gaps_of(r)]
            firsts = [first_of(r) for r in warm if first_of(r) is not None]
            cells += [
                f(pct(gaps, 0.5)),
                f(pct(gaps, 0.95)),
                f(max(gaps, default=0)),
                f(max((g for r in cold for g in gaps_of(r)), default=None)),
                "/".join(f(first_of(r)) for r in cold),
                f(statistics.median(firsts) if firsts else None),
            ]
        slides = [
            r["slide_max_gap"] for r in cold + warm if r["slide_max_gap"] is not None
        ]
        cells.append(f(max(slides) if slides else None))
        lines.append("| " + " | ".join(cells) + " |")
    return "\n".join(lines)


assert (
    pct([1, 2, 3, 4, 100], 0.5) == 3 and pct([5.0], 0.95) == 5.0 and pct([], 0.5) == 0.0
)


def main():
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("--analyze", type=Path, nargs="+")
    p.add_argument("--stage", type=Path)
    p.add_argument("--out", type=Path)
    p.add_argument("--pointer", type=Path, help="Bead's private-pointer example binary")
    p.add_argument("--niri", type=Path, default=Path("/usr/bin/niri"))
    p.add_argument("--render-node", type=Path, default=Path("/dev/dri/renderD129"))
    p.add_argument("--mem-rail-gib", type=float, default=3)
    p.add_argument("--settle-s", type=float, default=8)
    p.add_argument("--idle-s", type=float, default=0)
    p.add_argument("--panels", default="")
    p.add_argument("--repeats", type=int, default=10)
    p.add_argument("--hold-s", type=float, default=60)
    p.add_argument(
        "--icon-x",
        type=float,
        default=1762,
        help="bar status icon (the bell at 1920 px)",
    )
    p.add_argument("--bar-y", type=float, default=25)
    a = p.parse_args()
    if a.analyze:
        print(table(a.analyze))
        return
    a.stage, a.out, a.niri = a.stage.resolve(), a.out.resolve(), a.niri.resolve()
    a.out.mkdir(parents=True, exist_ok=True)
    Capture(a).run()


if __name__ == "__main__":
    main()
