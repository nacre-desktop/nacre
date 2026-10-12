#!/usr/bin/env python3
"""Private, atomic panel layout and assistant-view identity recovery."""

import json
import os
from pathlib import Path
import sys
import tempfile


def location():
    return Path(
        os.environ.get(
            "NACRE_PEARL_STATE",
            str(Path.home() / ".local/state/nacre/shell/pearls.json"),
        )
    )


def load(path):
    try:
        value = json.loads(path.read_text())
        if not isinstance(value, dict) or value.get("version") != 1:
            raise ValueError("Unsupported panel layout version")
        return value
    except FileNotFoundError:
        return {"version": 1, "windows": [], "ai": {"edge": "", "channels": [""]}}


def save(path, value):
    if (
        not isinstance(value, dict)
        or value.get("version") != 1
        or not isinstance(value.get("windows"), list)
    ):
        raise ValueError("Invalid panel layout")
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    descriptor, temporary = tempfile.mkstemp(dir=path.parent, prefix=".pearls-")
    try:
        with os.fdopen(descriptor, "w") as stream:
            json.dump(value, stream)
        os.replace(temporary, path)
    finally:
        Path(temporary).unlink(missing_ok=True)


def main():
    try:
        if sys.argv[1] == "save":
            save(location(), json.loads(sys.stdin.readline()))
        else:
            print(json.dumps(load(location())))
    except (OSError, ValueError, KeyError) as error:
        print(json.dumps({"error": str(error)}))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
