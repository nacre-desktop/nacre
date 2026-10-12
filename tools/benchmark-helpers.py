#!/usr/bin/env python3
"""Compare identical helper requests, fresh Python versus the persistent service."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

from performance import summarize


def benchmark(tools, count, service):
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
        try:
            for tool, arguments, payload in requests:
                timings = []
                outputs = []
                cold_ms = None
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
                # The dashboard's checked timestamp changes by design.
                for value in outputs:
                    if isinstance(value, dict):
                        value.pop("checked", None)
                if any(value != outputs[0] for value in outputs):
                    raise ValueError("Responses changed during the benchmark")
                results[tool] = {
                    "first_ms": cold_ms,
                    "warm_ms": summarize(timings),
                    "requests": count,
                    "response": outputs[0],
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
            "startup_ms": startup,
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
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not 1 <= args.count <= 10000:
        parser.error("count must be in 1..10000")
    args.output.write_text(
        json.dumps(benchmark(args.tools, args.count, args.service), indent=2) + "\n"
    )
    print(args.output)


if __name__ == "__main__":
    main()
