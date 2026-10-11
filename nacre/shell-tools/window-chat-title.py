#!/usr/bin/env python3
"""Resolve only a running terminal's named chat; never read auth/conversation bodies."""

import json
import os
import re
import sys
import unicodedata
from pathlib import Path


def resolve_info(pid):
    todo = [pid]
    seen = set()
    while todo and len(seen) < 64:
        current = todo.pop()
        if current in seen:
            continue
        seen.add(current)
        proc = Path("/proc") / str(current)
        try:
            if proc.stat().st_uid != os.getuid():
                continue
            todo.extend(
                map(
                    int, (proc / "task" / str(current) / "children").read_text().split()
                )
            )
            args = (proc / "cmdline").read_bytes().decode(errors="replace").split("\0")
            option = next(
                (arg for arg in ("resume", "--resume", "-r") if arg in args), None
            )
            if not option:
                continue
            thread = args[args.index(option) + 1]
            if not re.fullmatch(r"[0-9a-f-]{36}", thread):
                continue
            file = Path.home() / ".local/state/nacre/ai/chat-titles" / f"{thread}.json"
            title = ""
            if file.exists():
                data = json.loads(file.read_text())
                title = "".join(
                    c
                    for c in str(data["title"])
                    if not unicodedata.category(c).startswith("C")
                )[:100]
            return {"title": title, "threadId": thread}
        except (OSError, ValueError, IndexError, KeyError):
            continue
    return {"title": "", "threadId": ""}


def resolve(pid):
    return resolve_info(pid)["title"]


if __name__ == "__main__":
    print(json.dumps(resolve_info(int(sys.argv[1]))))
