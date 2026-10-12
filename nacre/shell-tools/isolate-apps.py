#!/usr/bin/env python3
"""Keep existing desktop applications alive when the shell service is restarted."""

import os
import subprocess
import time
from pathlib import Path

import compositor


def in_shell(pid):
    try:
        return (
            "/nacre-shell.service" in (Path("/proc") / str(pid) / "cgroup").read_text()
        )
    except OSError:
        return False


def descendants(pid):
    result = []
    queue = [pid]
    while queue:
        p = queue.pop()
        if p in result:
            continue
        root = Path("/proc") / str(p)
        try:
            if root.stat().st_uid != os.getuid():
                continue
            queue.extend(
                map(int, (root / "task" / str(p) / "children").read_text().split())
            )
            if in_shell(p):
                result.append(p)
        except OSError:
            continue
    return result


def main():
    data = compositor.query("windows", timeout=None)
    roots = [c["pid"] for c in data if in_shell(c["pid"])]
    for p in Path("/proc").iterdir():
        if not p.name.isdigit():
            continue
        try:
            if (
                p.stat().st_uid == os.getuid()
                and (p / "comm").read_text().strip() in ("kitty", "tmux: server")
                and in_shell(int(p.name))
            ):
                roots.append(int(p.name))
        except OSError:
            pass
    migrated = set()
    for pid in roots:
        if pid in migrated:
            continue
        pids = [p for p in descendants(pid) if p not in migrated]
        if not pids:
            continue
        argv = [
            "busctl",
            "--user",
            "call",
            "org.freedesktop.systemd1",
            "/org/freedesktop/systemd1",
            "org.freedesktop.systemd1.Manager",
            "StartTransientUnit",
            "ssa(sv)a(sa(sv))",
            f"nacre-app-{pid}-{time.time_ns()}.scope",
            "fail",
            "2",
            "PIDs",
            "au",
            str(len(pids)),
            *map(str, pids),
            "Description",
            "s",
            "Nacre desktop application",
            "0",
        ]
        subprocess.run(argv, check=True, stdout=subprocess.DEVNULL)
        migrated.update(pids)
    print("Protected application processes:", len(migrated))


if __name__ == "__main__":
    main()
