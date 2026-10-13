#!/usr/bin/env python3
"""Compare identical helper requests, fresh Python versus the persistent service."""

import argparse
import json
import os
from pathlib import Path
import resource
import subprocess
import sys
import tempfile
import time

from performance import memory, summarize


def cpu_times(worker=None):
    """User/system CPU, excluding waits and the benchmark driver's own CPU."""
    if worker:
        text = (Path("/proc") / str(worker.pid) / "stat").read_text()
        fields = text[text.rindex(")") + 2 :].split()
        ticks = os.sysconf("SC_CLK_TCK")
        # Include reaped descendants: helper-owned external work is still work.
        return (
            (int(fields[11]) + int(fields[13])) / ticks,
            (int(fields[12]) + int(fields[14])) / ticks,
        )
    usage = resource.getrusage(resource.RUSAGE_CHILDREN)
    return usage.ru_utime, usage.ru_stime


def benchmark(tools, count, service, busy_machine=False):
    requests = (
        ("menu-backend", ["convert", "invalid"], None),
        ("dashboard-data", ["read"], None),
        ("launcher-preferences", ["load"], None),
        ("notification-history", ["load"], None),
    )
    with tempfile.TemporaryDirectory(prefix="nacre-helper-bench-") as directory:
        environment = {
            **os.environ,
            "HOME": directory,
            "XDG_CONFIG_HOME": directory + "/.config",
            "XDG_STATE_HOME": directory + "/.local/state",
            "XDG_CACHE_HOME": directory + "/.cache",
            "XDG_DATA_HOME": directory + "/.local/share",
            "XDG_DATA_DIRS": "/usr/local/share:/usr/share",
        }
        worker = None
        if service:
            started = time.monotonic()
            worker = subprocess.Popen(
                [sys.executable, str(tools / "helper-service.py")],
                env=environment,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                text=True,
            )
            if json.loads(worker.stdout.readline()) != {"ready": True, "version": 1}:
                raise ValueError("Missing service handshake")
            startup = (time.monotonic() - started) * 1000
        else:
            startup = None
        results = {}
        service_memory = None
        try:
            for tool, arguments, payload in requests:
                timings = []
                outputs = []
                cold_ms = None
                cpu_start = None
                for sequence in range(count + 1):
                    started = time.monotonic()
                    if worker:
                        worker.stdin.write(
                            json.dumps(
                                {
                                    "id": sequence + 1,
                                    "tool": tool,
                                    "arguments": arguments,
                                    "input": payload,
                                }
                            )
                            + "\n"
                        )
                        worker.stdin.flush()
                        reply = json.loads(worker.stdout.readline())
                        if reply["id"] != sequence + 1 or reply["code"]:
                            raise ValueError("Unexpected helper response")
                        output = json.loads(reply["output"])
                    else:
                        reply = subprocess.run(
                            [sys.executable, str(tools / (tool + ".py")), *arguments],
                            env=environment,
                            capture_output=True,
                            text=True,
                            timeout=30,
                            check=True,
                        )
                        output = json.loads(reply.stdout)
                    elapsed = (time.monotonic() - started) * 1000
                    if sequence:
                        timings.append(elapsed)
                        outputs.append(output)
                    else:
                        cold_ms = elapsed
                        cpu_start = cpu_times(worker)
                cpu_end = cpu_times(worker)
                # The dashboard's checked timestamp changes by design.
                for value in outputs:
                    if isinstance(value, dict):
                        value.pop("checked", None)
                if any(value != outputs[0] for value in outputs):
                    raise ValueError("Responses changed during the benchmark")
                results[tool] = {
                    "requests": count,
                    "cpu_ms": {
                        "user": (cpu_end[0] - cpu_start[0]) * 1000,
                        "system": (cpu_end[1] - cpu_start[1]) * 1000,
                        "total_per_request": (
                            (cpu_end[0] - cpu_start[0]) + (cpu_end[1] - cpu_start[1])
                        )
                        * 1000
                        / count,
                    },
                    "response": outputs[0],
                }
                if not busy_machine:
                    results[tool].update(first_ms=cold_ms, warm_ms=summarize(timings))
            if worker:
                # Outside the timed requests; same smaps method as the sampler.
                service_memory = {
                    key + "_mib": value / (2**20) if value is not None else None
                    for key, value in memory(worker.pid).items()
                    if key
                    in (("rss",) if busy_machine else ("rss", "pss", "uss", "swap_pss"))
                }
        finally:
            if worker:
                worker.stdin.close()
                worker.wait(timeout=30)
                worker.stdout.close()
                if worker.returncode:
                    raise ValueError("Helper service did not exit cleanly")
        return {
            "transport": "service" if service else "fresh Python",
            "python_processes": 1 if service else len(requests) * (count + 1),
            "startup_ms": None if busy_machine else startup,
            "service_memory": service_memory,
            "measurement_condition": "busy machine"
            if busy_machine
            else "unspecified; verify a quiet window before timing claims",
            "cpu_scope": "helper and reaped descendants; user + system, excludes benchmark driver",
            "cpu_resolution_ms": 1000 / os.sysconf("SC_CLK_TCK") if service else 0.001,
            "results": results,
        }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--tools",
        type=Path,
        default=Path(__file__).resolve().parents[1] / "nacre/shell-tools",
    )
    parser.add_argument("--count", type=int, default=100)
    parser.add_argument("--service", action="store_true")
    parser.add_argument(
        "--busy-machine",
        action="store_true",
        help="Report CPU, launches and helper RSS; suppress wall-clock and PSS results",
    )
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not 1 <= args.count <= 10000:
        parser.error("count must be in 1..10000")
    args.output.write_text(
        json.dumps(
            benchmark(args.tools, args.count, args.service, args.busy_machine), indent=2
        )
        + "\n"
    )
    print(args.output)


if __name__ == "__main__":
    main()
