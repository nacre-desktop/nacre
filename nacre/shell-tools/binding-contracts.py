"""Capture declarative Lua bind contracts in a restricted, process-free environment."""

import json
from pathlib import Path
import shutil
import subprocess


def capture(path, kind="bindings"):
    if kind not in ("bindings", "rules"):
        raise ValueError("Unsupported declaration kind")
    lua = shutil.which("lua")
    if not lua:
        raise RuntimeError("Lua required for declarative binding checks")
    result = subprocess.run(
        [lua, str(Path(__file__).with_suffix(".lua")), str(path), kind],
        capture_output=True,
        text=True,
        timeout=10,
    )
    if result.returncode:
        raise RuntimeError(
            "Cannot capture bindings in " + path.name + ": " + result.stderr
        )
    records = json.loads(result.stdout)
    return records if isinstance(records, list) else []
