#!/usr/bin/env python3
"""Visible-only disk/process snapshot and identity-checked explicit termination."""

import json
import os
from pathlib import Path
import signal
import sys


def process_info(pid, proc=Path("/proc")):
    path = proc / str(pid)
    try:
        raw = (path / "stat").read_text()
        fields = raw[raw.rfind(")") + 2 :].split()
        return dict(
            pid=int(pid),
            name=raw[raw.find("(") + 1 : raw.rfind(")")][:80],
            start=fields[19],
            uid=path.stat().st_uid,
            memory=int(fields[21]) * os.sysconf("SC_PAGE_SIZE"),
            cpuSeconds=(int(fields[11]) + int(fields[12])) / os.sysconf("SC_CLK_TCK"),
            parent=int(fields[1]),
        )
    except (OSError, ValueError, IndexError):
        return None


def snapshot(proc=Path("/proc")):
    processes = []
    for path in proc.iterdir():
        if path.name.isdigit():
            item = process_info(int(path.name), proc)
            if item and item["uid"] == os.getuid():
                processes.append(item)
    disks = []
    seen = set()
    for line in (proc / "mounts").read_text().splitlines():
        fields = line.split()
        if len(fields) < 3 or not fields[0].startswith("/dev/"):
            continue
        mount = fields[1].replace("\\040", " ")
        try:
            device = os.stat(mount).st_dev
            if device in seen:
                continue
            seen.add(device)
            stats = os.statvfs(mount)
            total = stats.f_blocks * stats.f_frsize
            disks.append(
                dict(
                    path=mount,
                    total=total,
                    used=(stats.f_blocks - stats.f_bfree) * stats.f_frsize,
                )
            )
        except OSError:
            continue
    return dict(
        processes=sorted(processes, key=lambda item: item["memory"], reverse=True)[:12],
        disks=disks[:12],
    )


def terminate(pid, start, proc=Path("/proc")):
    item = process_info(pid, proc)
    if not item or item["start"] != str(start) or item["uid"] != os.getuid():
        raise ValueError("Process changed; refresh before ending it")
    ancestors = {os.getpid()}
    parent = os.getppid()
    for _ in range(20):
        ancestors.add(parent)
        info = process_info(parent, proc)
        if not info or info["parent"] <= 1:
            break
        parent = info["parent"]
    if pid <= 1 or pid in ancestors:
        raise ValueError("Cannot end the dashboard or its session")
    # Recheck identity immediately before signaling; Linux may recycle PIDs.
    # pidfd binds the request to the exact process even if it exits concurrently.
    descriptor = os.pidfd_open(pid)
    try:
        current = process_info(pid, proc)
        if not current or current["start"] != str(start):
            raise ValueError("Process changed")
        signal.pidfd_send_signal(descriptor, signal.SIGTERM)
    finally:
        os.close(descriptor)


if __name__ == "__main__":
    try:
        if len(sys.argv) > 1:
            if sys.argv[1] != "end":
                raise ValueError("Unknown action")
            terminate(int(sys.argv[2]), sys.argv[3])
        print(json.dumps(snapshot()))
    except (OSError, ValueError, KeyError, TypeError, IndexError) as error:
        print(json.dumps(dict(error=str(error)[:180])))
