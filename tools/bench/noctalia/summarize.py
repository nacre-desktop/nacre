#!/usr/bin/env python3
"""Aggregate bench.py runs: median [min, max] per arm, a scoreboard row, hotspots.

    summarize.py OUT_DIR [--label 'desktop 2026-11-13']

Reads OUT_DIR/*/result.json (one per completed run), writes OUT_DIR/table.md
and prints the scoreboard row plus the ranked thread hotspots.
"""

import argparse
import json
import statistics
from collections import defaultdict
from pathlib import Path

ARMS = ("noctalia", "nacre")
TREES = ("shell", "daemon", "niri", "labwc", "combined")
WINDOWS = ("idle", "interaction", "cycle1", "cycles_rest", "rest")
POINTS = ("startup", "pre-interaction", "post-interaction", "end")
# v0.1 row 73: Nacre at or under Noctalia on each (lower is better), plus every
# probed panel's first frame.
GATE = [
    ("idle CPU %", "idle.combined.cpu_pct"),
    ("idle wakeups/s", "idle.combined.ctx_per_s"),
    ("launch to bar s", "readiness_s"),
    (
        "memory before first use, PSS+GPU MiB",
        "memory.pre-interaction.combined.pss_gpu_mib",
    ),
    ("memory after first use, PSS+GPU MiB", "memory.end.combined.pss_gpu_mib"),
    ("interaction CPU %", "interaction.combined.cpu_pct"),
]
REPORTED = [
    ("PSS-only growth MiB", "memory_growth_mib"),
    ("interaction stack CPU %", "interaction.stack.cpu_pct"),
    ("interaction GPU %", "interaction.combined.gpu_pct"),
    (
        "interaction package energy above baseline J",
        "power.interaction.energy_j.above_baseline",
    ),
]


def flatten(result):
    flat = {"readiness_s": result["readiness_s"]}
    for name, data in result["windows"].items():
        for tree in TREES:
            if data.get(tree):
                for key in ("cpu_pct", "cpu_pct_ticks", "ctx_per_s", "gpu_pct"):
                    flat[f"{name}.{tree}.{key}"] = data[tree][key]
        # Everything the shell makes run: its trees plus the compositors it drives.
        flat[f"{name}.stack.cpu_pct"] = sum(
            data[tree]["cpu_pct"]
            for tree in ("shell", "daemon", "niri", "labwc")
            if data.get(tree)
        )
    for point, trees in result["memory"].items():
        for tree, values in trees.items():
            for key, value in values.items():
                flat[f"memory.{point}.{tree}.{key}"] = value
        # GL buffers sit outside PSS, so the gated memory is both (row 73).
        both = trees["combined"]
        flat[f"memory.{point}.combined.pss_gpu_mib"] = both["pss_mib"] + both["gpu_mib"]
    flat["memory_growth_mib"] = result["memory_growth_mib"]
    for name, data in result["power"].items():
        for key, values in data.items():
            flat[f"power.{name}.{key}.above_baseline"] = values["above_baseline"]
    ppt = result["power"].get("interaction", {}).get("ppt_uw")
    seconds = (result["windows"].get("interaction", {}).get("shell") or {}).get(
        "seconds"
    )
    if (
        ppt and seconds
    ):  # package energy the interaction adds over the no-shell baseline
        flat["power.interaction.energy_j.above_baseline"] = (
            ppt["above_baseline"] * seconds
        )
    for panel, probes in result["open_frames"].items():
        for key in ("first_ms", "frames", "max_gap_ms", "late"):
            seen = [p[key] for p in probes if p]
            flat[f"frames.{panel}.{key}"] = statistics.median(seen) if seen else None
    flat["host_swapin_mib"] = result["host_swapin_mib"]
    if not result.get("render_node"):  # software rendering: no GPU counters or power
        flat = {k: v for k, v in flat.items() if "gpu" not in k and "power" not in k}
    return flat


def stats(values):
    seen = [v for v in values if v is not None]
    if not seen:
        return None
    return (statistics.median(seen), min(seen), max(seen), len(seen))


def cell(entry, digits=3):
    if entry is None:
        return "n/a"
    median, low, high, _ = entry
    return f"{median:.{digits}f} [{low:.{digits}f}, {high:.{digits}f}]"


def hotspots(results, window, limit=10):
    """Median per tree/process/thread CPU (% of a core) and wakeups/s across runs of one arm."""
    cpu, wake, pool = defaultdict(list), defaultdict(list), defaultdict(list)
    for result in results:
        data = result["windows"].get(window, {})
        seconds = next((v["seconds"] for v in data.values() if v and "seconds" in v), 0)
        for row in data.get("threads", {}).get("by_cpu", []):
            name = f"{row['tree']}/{row['process']}/{row['thread']}"
            cpu[name].append(row["cpu_ms"] / 10 / seconds if seconds else 0)
            pool[name].append((row["threads"], row.get("spawned")))
        for row in data.get("threads", {}).get("by_wakeups", []):
            wake[f"{row['tree']}/{row['process']}/{row['thread']}"].append(
                row["ctx_per_s"]
            )

    def top(table):
        # A thread missing from a run's top list counts as 0 there.
        ranked = {
            k: statistics.median(v + [0] * (len(results) - len(v)))
            for k, v in table.items()
        }
        return sorted(ranked.items(), key=lambda kv: -kv[1])[:limit]

    counts = {  # median most alive at once, median spawned in the window
        k: tuple(
            statistics.median(x) if None not in x else "?" for x in zip(*v, strict=True)
        )
        for k, v in pool.items()
    }
    return top(cpu), top(wake), counts


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("out", type=Path)
    parser.add_argument("--label", default="")
    args = parser.parse_args()
    runs = defaultdict(list)
    for path in sorted(args.out.glob("*/result.json")):
        status = (path.parent / "status.txt").read_text().strip()
        if status != "PASS":
            print(f"skipping {path.parent.name}: {status}")
            continue
        result = json.loads(path.read_text())
        runs[result["arm"]].append(result)
    if not runs:
        raise SystemExit("no passing runs under " + str(args.out))
    flat = {arm: [flatten(r) for r in rs] for arm, rs in runs.items()}
    metrics = []
    for arm in ARMS:
        for row in flat.get(arm, []):
            metrics += [k for k in row if k not in metrics]
    table = {
        arm: {m: stats([r.get(m) for r in flat.get(arm, [])]) for m in metrics}
        for arm in ARMS
    }
    lines = ["| Metric | Noctalia | Nacre |", "| --- | ---: | ---: |"]
    for metric in metrics:
        lines.append(
            f"| {metric} | {cell(table['noctalia'][metric])} | {cell(table['nacre'][metric])} |"
        )
    (args.out / "table.md").write_text("\n".join(lines) + "\n")
    print(f"wrote {args.out / 'table.md'} ({len(metrics)} metrics)")

    def pair(metric, digits=2):
        return " / ".join(
            "n/a"
            if table[arm].get(metric) is None
            else f"{table[arm][metric][0]:.{digits}f}"
            for arm in ARMS
        )

    pins = next(iter(runs.values()))[0].get("pins", {})
    counts = "/".join(str(len(runs.get(arm, []))) for arm in ARMS)
    row = (
        f"| {args.label} | {counts} | {pins.get('nacre', '?')[:9]} | {pins.get('noctalia_tag', '?')} "
        f"| {pair('idle.combined.cpu_pct', 3)} | {pair('idle.niri.cpu_pct', 3)} "
        f"| {pair('idle.combined.ctx_per_s', 1)} "
        f"| {pair('interaction.combined.cpu_pct', 1)} | {pair('interaction.stack.cpu_pct', 1)} "
        f"| {pair('interaction.combined.gpu_pct', 1)} "
        f"| {pair('memory.pre-interaction.combined.pss_mib', 0)} "
        f"| {pair('memory.pre-interaction.combined.unreclaimable_mib', 0)} "
        f"| {pair('memory.pre-interaction.combined.gpu_mib', 0)} "
        f"| {pair('memory.pre-interaction.combined.pss_gpu_mib', 0)} "
        f"| {pair('memory.end.combined.pss_gpu_mib', 0)} | {pair('memory_growth_mib', 1)} "
        f"| {pair('readiness_s', 3)} | {pair('frames.launcher.first_ms', 0)} "
        f"| {pair('frames.launcher.late', 0)} "
        f"| {pair('power.idle.ppt_uw.above_baseline')} | {pair('power.interaction.ppt_uw.above_baseline')} "
        f"| {pair('power.interaction.energy_j.above_baseline', 0)} |"
    )
    print("\nscoreboard row (Noctalia / Nacre):")
    print(row)
    panels = [
        m.split(".")[1]
        for m in metrics
        if m.startswith("frames.") and m.endswith(".first_ms")
    ]
    print("\npanel open latency row (first frame ms, late frames; Noctalia / Nacre):")
    print(
        f"| {args.label} | {counts} | {pins.get('nacre', '?')[:9]} | "
        + " | ".join(
            f"{pair(f'frames.{p}.first_ms', 0)} ({pair(f'frames.{p}.late', 0)})"
            for p in panels
        )
        + " |  ← "
        + ", ".join(panels)
    )
    print("\nrow 73 gate (medians, Noctalia / Nacre; Nacre must be at or under):")
    gate = GATE + [
        (f"{p} open, first frame ms", f"frames.{p}.first_ms") for p in panels
    ]
    for label, metric in gate + REPORTED:
        values = [table[arm].get(metric) for arm in ARMS]
        verdict = (
            "report"
            if (label, metric) in REPORTED
            else "n/a"
            if None in values
            else "PASS"
            if values[1][0] <= values[0][0]
            else "FAIL"
        )
        print(f"  {verdict:6} {label}: {pair(metric, 3)}")
    for arm in ARMS:
        if not runs.get(arm):
            continue
        for window in ("idle", "interaction"):
            cpu, wake, counts = hotspots(runs[arm], window)
            print(f"\n{arm} {window}: top threads by CPU (median % of one core)")
            for name, value in cpu:
                alive, spawned = counts[name]
                print(f"  {value:8.3f} %  {name} (alive {alive}, spawned {spawned})")
            print(f"{arm} {window}: top threads by wakeups (median ctx/s)")
            for name, value in wake:
                print(f"  {value:8.2f} /s {name}")


if __name__ == "__main__":
    main()
