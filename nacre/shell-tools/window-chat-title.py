#!/usr/bin/env python3
"""Resolve only a running terminal's named chat; never read auth/conversation bodies."""

import json
import os
import re
import sys
import unicodedata
from pathlib import Path

PROC = Path("/proc")


def clean_title(value):
    return " ".join(
        "".join(
            c for c in str(value) if not unicodedata.category(c).startswith("C")
        ).split()
    )[:100]


def process_args(pid):
    proc = PROC / str(pid)
    if proc.stat().st_uid != os.getuid():
        raise ValueError("Process belongs to another user")
    return (proc / "cmdline").read_bytes().decode(errors="replace").split("\0")


def chat_identity(args):
    if not args or Path(args[0]).name not in ("codex", "claude"):
        return "", ""
    option = next((arg for arg in ("resume", "--resume", "-r") if arg in args), None)
    thread = args[args.index(option) + 1] if option else ""
    if thread and not re.fullmatch(r"[0-9a-f-]{36}", thread):
        thread = ""
    cwd = args[args.index("-C") + 1] if "-C" in args else ""
    return thread, cwd


def cached_info(thread, cwd):
    root = Path.home() / ".local/state/nacre/ai/chat-titles"
    if thread:
        files = [root / f"{thread}.json"]
    else:
        # A fresh general chat has its own directory. Never guess among several
        # unrelated conversations running in the same project checkout.
        directory = Path(cwd)
        chat_root = Path.home() / ".local/share/nacre/ai/chats"
        if directory.parent != chat_root or not re.fullmatch(
            r"[0-9a-f]{32}", directory.name
        ):
            return {"title": "", "threadId": ""}
        try:
            identity = json.loads(
                (root / "directories" / f"{directory.name}.json").read_text()
            )["threadId"]
            if re.fullmatch(r"[0-9a-f-]{36}", identity):
                return cached_info(identity, "")
        except (OSError, ValueError, KeyError, TypeError):
            pass
        files = list(root.glob("*.json"))
    matches = []
    for file in files:
        try:
            data = json.loads(file.read_text())
            if thread or data.get("cwd") == cwd:
                matches.append(
                    {"title": clean_title(data["title"]), "threadId": file.stem}
                )
        except (OSError, ValueError, KeyError):
            continue
    return matches[0] if len(matches) == 1 else {"title": "", "threadId": ""}


def resolve_info(pid):
    todo = [pid]
    seen = set()
    while todo and len(seen) < 64:
        current = todo.pop()
        if current in seen:
            continue
        seen.add(current)
        proc = PROC / str(current)
        try:
            if proc.stat().st_uid != os.getuid():
                continue
            todo.extend(
                map(
                    int, (proc / "task" / str(current) / "children").read_text().split()
                )
            )
            thread, cwd = chat_identity(process_args(current))
            if thread or cwd:
                info = cached_info(thread, cwd)
                if info["title"]:
                    return info
                if thread:
                    return {"title": "", "threadId": thread}
        except (OSError, ValueError, IndexError, KeyError):
            continue
    return {"title": "", "threadId": ""}


def resolve(pid):
    return resolve_info(pid)["title"]


if __name__ == "__main__":
    print(json.dumps(resolve_info(int(sys.argv[1]))))
