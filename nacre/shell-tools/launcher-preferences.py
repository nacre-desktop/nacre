#!/usr/bin/env python3
"""Private launcher favorites/hidden IDs; atomic updates preserve concurrent edits."""

import argparse
import fcntl
import json
import os
import tempfile
import sys
import time
from pathlib import Path

PATH = Path.home() / ".config/nacre/launcher.json"


def load():
    try:
        data = json.loads(PATH.read_text())
    except FileNotFoundError:
        data = {}
    if not isinstance(data, dict) or any(
        not isinstance(data.get(key, []), list)
        for key in ("favorites", "hidden", "recent")
    ):
        raise ValueError("Invalid launcher preferences; original file preserved")
    result = {
        key: list(dict.fromkeys(x for x in data.get(key, []) if isinstance(x, str)))
        for key in ("favorites", "hidden", "recent")
    }

    usage = data.get("usage", {})
    result["usage"] = {
        ident: {"count": min(1000000, entry["count"]), "last": entry["last"]}
        for ident, entry in (usage.items() if isinstance(usage, dict) else [])
        if isinstance(ident, str)
        and len(ident) <= 256
        and isinstance(entry, dict)
        and type(entry.get("count")) is int
        and entry["count"] > 0
        and type(entry.get("last")) in (int, float)
        and 0 <= entry["last"] <= time.time() + 60
    }
    return result


def change(action, value):
    key = {"favorite": "favorites", "hide": "hidden", "record": "recent"}[action]
    ident = value.get("id")
    enabled = value.get("enabled")
    if (
        not isinstance(ident, str)
        or not ident
        or len(ident) > 256
        or any(ord(c) < 32 for c in ident)
        or type(enabled) is not bool
    ):
        raise ValueError("Invalid launcher preference")
    PATH.parent.mkdir(parents=True, exist_ok=True)
    with PATH.with_suffix(".lock").open("w") as guard:
        os.chmod(guard.name, 0o600)
        fcntl.flock(guard, fcntl.LOCK_EX)
        data = load()
        data[key] = [x for x in data[key] if x != ident]
        if enabled:
            if action == "record":
                data[key] = [ident, *data[key]][:12]
                old = data["usage"].get(ident, {})
                data["usage"][ident] = {
                    "count": min(1000000, old.get("count", 0) + 1),
                    "last": time.time(),
                }
                data["usage"] = dict(
                    sorted(
                        data["usage"].items(),
                        key=lambda item: item[1]["last"],
                        reverse=True,
                    )[:512]
                )
            else:
                data[key].append(ident)
        fd, name = tempfile.mkstemp(dir=PATH.parent)
        try:
            with os.fdopen(fd, "w") as stream:
                json.dump(data, stream)
            os.chmod(name, 0o600)
            os.replace(name, PATH)
        finally:
            if os.path.exists(name):
                os.unlink(name)
        return data


def initialize():
    if not PATH.exists():
        PATH.parent.mkdir(parents=True, exist_ok=True)
        try:
            with PATH.open("x") as stream:
                json.dump({"favorites": [], "hidden": []}, stream)
            PATH.chmod(0o600)
        except FileExistsError:
            pass
    return load()


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("action", choices=["load", "favorite", "hide", "record"])
    a = p.parse_args()
    try:
        print(
            json.dumps(
                initialize()
                if a.action == "load"
                else change(a.action, json.loads(sys.stdin.readline()))
            )
        )
    except (OSError, ValueError, KeyError, TypeError) as e:
        print(json.dumps({"error": str(e)}))
        sys.exit(1)
