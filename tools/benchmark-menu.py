#!/usr/bin/env python3
"""Real menu.query on the existing private nested-session lab, without a UI frame claim."""

import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import statistics
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location(
    "menu_daemon_lab", ROOT / "tools/benchmark-daemon.py"
)
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)


def children(pid):
    result = set()
    for task in Path(f"/proc/{pid}/task").iterdir():
        result.update(int(value) for value in (task / "children").read_text().split())
    return result


def call(link, sequence, method, params):
    started = time.perf_counter_ns()
    link.send(dict(t="call", id=sequence, m=method, p=params, timeout=10000))
    reply = link.receive()
    elapsed = (time.perf_counter_ns() - started) / 1_000_000
    if reply.get("t") != "ret" or reply.get("id") != sequence:
        raise RuntimeError("Launcher reply ID mismatch")
    if "err" in reply:
        raise RuntimeError("Launcher call failed: " + json.dumps(reply["err"]))
    return reply["ok"], elapsed


def measure(tools, env, options, output):
    runtime = Path(env["XDG_RUNTIME_DIR"])
    home = Path(env["HOME"])
    if home == Path.home() or not str(runtime).startswith("/tmp/npr-"):
        raise RuntimeError("This benchmark requires the private nested lab")
    data = home / "menu-data"
    apps = data / "applications"
    apps.mkdir(parents=True)
    empty = home / "empty-data"
    empty.mkdir()
    endpoint = runtime / "nacre/daemon.sock"
    environment = dict(
        env,
        NACRE_DAEMON_SOCKET=str(endpoint),
        XDG_DATA_HOME=str(data),
        XDG_DATA_DIRS=str(empty),
        NACRE_NIRI_HOVER_STREAM="0",
        DBUS_SESSION_BUS_ADDRESS="unix:path=" + str(runtime / "absent-session-bus"),
    )
    helper = tools / "nacre-helper"
    reports = []
    for count in MENU_SIZES:
        for path in apps.glob("*.desktop"):
            path.unlink()
        for number in range(count):
            (apps / f"fixture-{number:04}.desktop").write_text(
                f"[Desktop Entry]\nType=Application\nName=Application {number:04}\nGenericName=Fixture tool\nComment=Synthetic application catalog\nExec=fixture-command {number}\nIcon=fixture\nCategories=Utility;\nKeywords=fixture;app;\n"
            )
        log = output.parent / f"{output.stem}-menu-{count}.log"
        with log.open("wb") as errors:
            daemon = subprocess.Popen(
                [str(helper), "daemon"], env=environment, stdout=errors, stderr=errors
            )
            link = None
            try:
                deadline = time.monotonic() + 20
                while time.monotonic() < deadline:
                    if daemon.poll() is not None:
                        raise RuntimeError("Private daemon exited before indexing")
                    if endpoint.exists():
                        if link is None:
                            link = bench.Link(endpoint)
                        link.send(
                            dict(
                                t="call",
                                id=1,
                                m="menu.query",
                                p=dict(query="", mode="apps", route="apps"),
                            )
                        )
                        ready = link.receive()
                        if "ok" in ready and len(ready["ok"].get("order", [])) == count:
                            break
                    time.sleep(0.05)
                else:
                    raise RuntimeError("Real catalog did not become ready")
                while time.monotonic() < deadline:
                    state, _ = call(link, 2, "daemon.status", {})
                    status = state.get("services", {}).get("menu", {}).get("status")
                    if status == "running":
                        break
                    if status == "degraded":
                        raise RuntimeError(
                            "Initial providers failed; discard this capture"
                        )
                    time.sleep(0.05)
                else:
                    raise RuntimeError("Initial providers did not become ready")
                initial = ready["ok"]
                if (
                    initial["order"][0] != "fixture-0000"
                    or initial["order"][-1] != f"fixture-{count - 1:04}"
                ):
                    raise RuntimeError("Indexed catalog order mismatch")
                queries = [
                    "a",
                    "ap",
                    "app",
                    "appl",
                    "application",
                    "application 0",
                    "fixture",
                    "tool",
                    "zzzz",
                ]
                for sequence in range(100):
                    call(
                        link,
                        sequence + 10,
                        "menu.query",
                        dict(
                            query=queries[sequence % len(queries)],
                            mode="apps",
                            route="apps",
                        ),
                    )
                pids = [daemon.pid, *sorted(children(daemon.pid))]
                before = bench.paging()
                starts = {pid: bench.lab.performance.process(pid) for pid in pids}
                memories = [bench.lab.performance.memory(pid) for pid in pids]
                samples = []
                for sequence in range(MENU_CALLS):
                    query = queries[sequence % len(queries)]
                    result, elapsed = call(
                        link,
                        sequence + 1000,
                        "menu.query",
                        dict(query=query, mode="apps", route="apps"),
                    )
                    if (
                        result["query"] != query
                        or not isinstance(result.get("items"), dict)
                        or result["catalog_rev"] != initial["catalog_rev"]
                    ):
                        raise RuntimeError(
                            "Pending, superseded or changed-catalog reply"
                        )
                    if any(item not in result["items"] for item in result["order"]):
                        raise RuntimeError("Result order points outside its collection")
                    samples.append(elapsed)
                    if sequence % 100 == 0:
                        memories.extend(
                            bench.lab.performance.memory(pid) for pid in pids
                        )
                ends = {pid: bench.lab.performance.process(pid) for pid in pids}
                after = bench.paging()
                samples.sort()
                cpu = {}
                for pid in pids:
                    if (
                        not starts[pid]
                        or not ends[pid]
                        or starts[pid]["start"] != ends[pid]["start"]
                    ):
                        raise RuntimeError("Measured process identity changed")
                    cpu[str(pid)] = (
                        (ends[pid]["ticks"] - starts[pid]["ticks"])
                        * 1000
                        / os.sysconf("SC_CLK_TCK")
                        / MENU_CALLS
                    )
                valid = (
                    all(row["swap_pss"] == 0 for row in memories)
                    and (after["pswpin"] - before["pswpin"])
                    * os.sysconf("SC_PAGE_SIZE")
                    < 1_048_576
                )
                row = dict(
                    catalog=count,
                    count=MENU_CALLS,
                    valid=valid,
                    completed_catalog_rev=initial["catalog_rev"],
                    ready_result_count=count,
                    p50_ms=statistics.median(samples),
                    p99_ms=samples[math.ceil(0.99 * len(samples)) - 1],
                    daemon_cpu_ms_per_query=cpu[str(daemon.pid)],
                    provider_cpu_ms_per_query=sum(
                        value for pid, value in cpu.items() if pid != str(daemon.pid)
                    ),
                    cpu_quantum_ms_per_query=1000
                    / os.sysconf("SC_CLK_TCK")
                    / MENU_CALLS,
                    helper_process_count=len(pids),
                    host_swap_before=before,
                    host_swap_after=after,
                    measured_swap_pss_max=max(row["swap_pss"] for row in memories),
                    busy_memory_mib={
                        key: sum(bench.lab.performance.memory(pid)[key] for pid in pids)
                        / 1_048_576
                        for key in ("rss", "pss")
                    },
                )
                # Worker retirement and blocking-pool retirement are allowed timers;
                # the following idle capture does not invoke any menu calls.
                time.sleep(options.daemon_settle)
                idle_before = bench.paging()
                idle_start = bench.lab.performance.process(daemon.pid)
                first = bench.switches(daemon.pid)
                idle_memories = []
                started = time.monotonic()
                while time.monotonic() - started < options.daemon_idle:
                    idle_memories.append(bench.lab.performance.memory(daemon.pid))
                    time.sleep(0.25)
                idle_end = bench.lab.performance.process(daemon.pid)
                last = bench.switches(daemon.pid)
                idle_after = bench.paging()
                row["idle"] = dict(
                    seconds=time.monotonic() - started,
                    cpu_ticks=idle_end["ticks"] - idle_start["ticks"],
                    ticks_per_second=os.sysconf("SC_CLK_TCK"),
                    threads=last["threads"],
                    helper_process_count=1 + len(children(daemon.pid)),
                    voluntary_switches=last["voluntary"] - first["voluntary"],
                    memory_mib={
                        key: statistics.median(row[key] for row in idle_memories)
                        / 1_048_576
                        for key in ("rss", "pss")
                    },
                    host_swap_before=idle_before,
                    host_swap_after=idle_after,
                    measured_swap_pss_max=max(row["swap_pss"] for row in idle_memories),
                    valid=all(row["swap_pss"] == 0 for row in idle_memories)
                    and (idle_after["pswpin"] - idle_before["pswpin"])
                    * os.sysconf("SC_PAGE_SIZE")
                    < 1_048_576,
                )
                reports.append(row)
            finally:
                if link:
                    link.close()
                bench.stop(daemon)
    output.write_text(
        json.dumps(
            dict(
                condition="desktop-PC nested helper microbenchmark; busy machine, not physical UI latency",
                capture_role="profile"
                if MENU_CALLS >= 2000
                and options.daemon_idle >= 60
                and options.daemon_settle >= 31
                else "functional-smoke",
                page_size=os.sysconf("SC_PAGE_SIZE"),
                backend=env["NACRE_COMPOSITOR"],
                helper_sha256=hashlib.sha256(helper.read_bytes()).hexdigest(),
                method="One outstanding request, send to matching completed reply including JSON decode; /proc CPU ticks include separate provider workers; no UI, compilation or catalog writes during samples",
                python_launches=None,
                python_launch_proof="Not exec-traced; native empty-PATH and worker lifecycle tests are separate evidence",
                samples=reports,
            ),
            indent=2,
        )
        + "\n"
    )


def main():
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("--menu-calls", type=int, default=2000)
    parser.add_argument("--menu-catalogs", default="320,1000,2000")
    args, remaining = parser.parse_known_args()
    global MENU_CALLS, MENU_SIZES
    MENU_CALLS = args.menu_calls
    MENU_SIZES = [int(value) for value in args.menu_catalogs.split(",")]
    if (
        MENU_CALLS < 100
        or not MENU_SIZES
        or any(value < 1 or value > 2048 for value in MENU_SIZES)
    ):
        parser.error("Invalid menu count or catalog sizes")
    sys.argv = [sys.argv[0], *remaining]
    bench.measure = measure
    bench.main()


if __name__ == "__main__":
    main()
