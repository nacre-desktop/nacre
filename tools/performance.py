#!/usr/bin/env python3
"""Sample Nacre CPU/RSS/PSS/USS, Python launches and IPC completion latency without sudo."""

import argparse
from collections import Counter
import json
import os
from pathlib import Path
import statistics
import subprocess
import time


def process(pid, proc=Path("/proc")):
    try:
        directory = proc / str(pid)
        stat = (directory / "stat").read_text()
        fields = stat[stat.rindex(")") + 2 :].split()
        args = (directory / "cmdline").read_bytes().split(b"\0")
        return {
            "pid": int(pid),
            "ppid": int(fields[1]),
            "start": int(fields[19]),
            "ticks": int(fields[11]) + int(fields[12]),
            "child_ticks": int(fields[13]) + int(fields[14]),
            "rss": int(fields[21]) * os.sysconf("SC_PAGE_SIZE"),
            "name": stat[stat.index("(") + 1 : stat.rindex(")")],
            "args": args,
            "uid": directory.stat().st_uid,
        }
    except (OSError, ValueError, IndexError):
        return None


def memory(pid, proc=Path("/proc")):
    """Read resident memory; unavailable rollups are unknown, never zero."""
    try:
        fields = {}
        for line in (proc / str(pid) / "smaps_rollup").read_text().splitlines():
            parts = line.split()
            if len(parts) == 3 and parts[2] == "kB":
                fields[parts[0].rstrip(":")] = int(parts[1]) * 1024
        return {
            "rss": fields["Rss"],
            "pss": fields["Pss"],
            "uss": fields["Private_Clean"] + fields["Private_Dirty"],
            "swap_pss": fields.get("SwapPss", 0),
            "memory_error": None,
        }
    except (OSError, ValueError, KeyError) as error:
        return {
            "pss": None,
            "uss": None,
            "swap_pss": None,
            "memory_error": type(error).__name__,
        }


def memory_sample(rows, shell_pid):
    sample = {}
    for metric in ("rss", "pss", "uss", "swap_pss"):
        sample["shell_" + metric + "_mib"] = (
            rows[shell_pid][metric] / 1048576
            if rows[shell_pid].get(metric) is not None
            else None
        )
        values = [row.get(metric) for row in rows.values()]
        sample["all_" + metric + "_mib"] = (
            sum(values) / 1048576
            if all(value is not None for value in values)
            else None
        )
    return sample


def snapshot(shell_pid, proc=Path("/proc"), detached=True):
    rows = {
        row["pid"]: row
        for path in proc.iterdir()
        if path.name.isdigit() and (row := process(path.name, proc))
    }
    selected = {shell_pid}
    while True:
        children = {pid for pid, row in rows.items() if row["ppid"] in selected}
        if children <= selected:
            break
        selected |= children
    # Include detached Nacre workers, but exclude this measurement process.
    prefixes = [
        bytes(str(Path.home() / ".local/share/nacre" / folder) + "/", "utf-8")
        for folder in ("shell", "brain")
    ]
    for pid, row in rows.items():
        if detached and row["uid"] == os.getuid() and pid != os.getpid():
            if any(
                any(arg.startswith(prefix) for prefix in prefixes)
                for arg in row["args"]
            ):
                selected.add(pid)
    result = {}
    for pid in selected:
        if pid not in rows:
            continue
        row = rows[pid]
        row.update(memory(pid, proc))
        # A process may exit or its PID may be reused while reading the rollup.
        current = process(pid, proc)
        if current and current["start"] == row["start"]:
            result[pid] = row
    return result


def summarize(values):
    if not values:
        return None
    ordered = sorted(values)
    return {
        "mean": statistics.mean(values),
        "median": statistics.median(values),
        "p95": ordered[min(len(values) - 1, int(len(values) * 0.95))],
        "max": max(values),
    }


def idle(shell_pid, duration, interval, spawn_log=None, detached=True):
    ticks_per_second = os.sysconf("SC_CLK_TCK")
    first = snapshot(shell_pid, detached=detached)
    if shell_pid not in first:
        raise ValueError("Shell PID is not running")
    identity = first[shell_pid]["start"]
    seen = {(pid, row["start"]) for pid, row in first.items()}
    previous = first
    cpu = {"shell": [], "all": []}
    resident = {
        metric: {"shell": [], "all": []} for metric in ("rss", "pss", "uss", "swap_pss")
    }
    names = Counter()
    spawns = [0] * max(1, int((duration + 59) // 60))
    samples = []
    start = before = time.monotonic()
    boot_start = time.clock_gettime(time.CLOCK_BOOTTIME)
    log_offset = spawn_log.stat().st_size if spawn_log and spawn_log.exists() else 0
    while time.monotonic() - start < duration:
        time.sleep(min(interval, max(0, duration - (time.monotonic() - start))))
        now = time.monotonic()
        rows = snapshot(shell_pid, detached=detached)
        if shell_pid not in rows or rows[shell_pid]["start"] != identity:
            raise ValueError("Shell exited or restarted; repeat the comparison")
        changes = {}
        for pid, row in rows.items():
            key = (pid, row["start"])
            if key not in seen:
                seen.add(key)
                if Path(row["args"][0].decode(errors="replace")).name.startswith(
                    "python"
                ):
                    spawns[min(len(spawns) - 1, int((now - start) // 60))] += 1
            old = previous.get(pid)
            changes[pid] = (
                max(0, row["ticks"] - old["ticks"])
                if old and old["start"] == row["start"]
                else row["ticks"]
            )
            names[row["name"]] += 1
        sample = {
            "elapsed_s": now - start,
            "shell_cpu_percent": changes[shell_pid]
            / ticks_per_second
            / (now - before)
            * 100,
            "all_cpu_percent": sum(changes.values())
            / ticks_per_second
            / (now - before)
            * 100,
            **memory_sample(rows, shell_pid),
            "processes": [
                {
                    "pid": pid,
                    "start_ticks": row["start"],
                    "name": row["name"],
                    "cpu_percent": changes[pid]
                    / ticks_per_second
                    / (now - before)
                    * 100,
                    **{
                        metric + "_mib": row[metric] / 1048576
                        if row.get(metric) is not None
                        else None
                        for metric in resident
                    },
                    "memory_error": row["memory_error"],
                }
                for pid, row in rows.items()
            ],
        }
        samples.append(sample)
        for scope in cpu:
            cpu[scope].append(sample[scope + "_cpu_percent"])
            for metric in resident:
                value = sample[scope + "_" + metric + "_mib"]
                if value is not None:
                    resident[metric][scope].append(value)
        previous, before = rows, now
    coverage = (
        "sampled lower bound; processes shorter than sample interval may be missed"
    )
    if spawn_log:
        spawns = [0] * len(spawns)
        with spawn_log.open() as stream:
            stream.seek(log_offset)
            for line in stream:
                launched = float(line.split()[0]) - boot_start
                if 0 <= launched < duration:
                    spawns[min(len(spawns) - 1, int(launched // 60))] += 1
        coverage = "PATH python3 launch log; absolute interpreter paths are excluded"
    return {
        "duration_s": before - start,
        "interval_s": interval,
        "cpu_percent_one_core": {key: summarize(value) for key, value in cpu.items()},
        **{
            metric + "_mib": {key: summarize(value) for key, value in scopes.items()}
            for metric, scopes in resident.items()
        },
        "memory_valid_samples": {
            metric: {key: len(value) for key, value in scopes.items()}
            for metric, scopes in resident.items()
        },
        "python_spawns_per_minute": spawns,
        "spawn_coverage": coverage,
        "process_names": sorted(names),
        "samples": samples,
    }


def call(pid, target, method, *arguments):
    result = subprocess.run(
        ["quickshell", "ipc", "--pid", str(pid), "call", target, method, *arguments],
        capture_output=True,
        text=True,
        timeout=10,
        check=True,
    )
    return result.stdout.strip()


def ready(state, expected):
    return all(state.get(key) == value for key, value in expected.items())


def actions(pid, specifications, repeats, spawn_log=None):
    results = {}
    for name, spec in specifications.items():
        values = []
        launches = []
        for _ in range(repeats):
            for command in spec.get("prepare", []):
                call(pid, *command)
            time.sleep(spec.get("settle_s", 0.25))
            started = time.monotonic()
            offset = spawn_log.stat().st_size if spawn_log else 0
            try:
                call(pid, *spec["call"])
                while True:
                    checks = spec.get("checks", [spec])
                    if all(
                        ready(json.loads(call(pid, *check["state"])), check["expected"])
                        for check in checks
                    ):
                        break
                    if time.monotonic() - started > spec.get("timeout_s", 30):
                        raise TimeoutError(name + " did not reach the requested state")
                    time.sleep(0.01)
                values.append((time.monotonic() - started) * 1000)
                if spawn_log:
                    with spawn_log.open() as stream:
                        stream.seek(offset)
                        launches.append(len(stream.readlines()))
            finally:
                for command in spec.get("cleanup", []):
                    call(pid, *command)
        results[name] = {
            "ms": summarize(values),
            "runs_ms": values,
            "python_launches": launches,
            "method": spec,
        }
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pid", required=True, type=int)
    parser.add_argument("--duration", type=float, default=300)
    parser.add_argument("--interval", type=float, default=0.05)
    parser.add_argument("--spawn-log", type=Path)
    parser.add_argument("--descendants-only", action="store_true")
    parser.add_argument(
        "--actions", type=Path, help="JSON IPC calls and ready predicates"
    )
    parser.add_argument("--repeats", type=int, default=10)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    if args.duration <= 0 or args.interval <= 0 or args.repeats <= 0:
        parser.error("duration, interval and repeats must be positive")
    report = {
        "version": 2,
        "date": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "shell_pid": args.pid,
        "scope": "shell descendants"
        if args.descendants_only
        else "shell descendants and detached Nacre shell/brain workers of current UID",
        "idle": idle(
            args.pid,
            args.duration,
            args.interval,
            args.spawn_log,
            not args.descendants_only,
        ),
        "actions": actions(
            args.pid, json.loads(args.actions.read_text()), args.repeats, args.spawn_log
        )
        if args.actions
        else {},
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(args.output)


if __name__ == "__main__":
    main()
