"""Developer-only adapter preserving retired pearl-state assertion bodies."""

import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
BINARY = Path(
    os.environ.get("NACRE_HELPER_BINARY") or ROOT / "target/debug/nacre-helper"
)


def command():
    return [str(BINARY), "pearl-state"]


def call(action, path, value=None):
    result = subprocess.run(
        [*command(), action],
        input=json.dumps(value) + "\n" if action == "save" else None,
        env={**os.environ, "NACRE_PEARL_STATE": str(path)},
        text=True,
        capture_output=True,
        timeout=5,
    )
    if result.returncode:
        message = json.loads(result.stdout)["error"]
        raise ValueError(message)
    if result.stderr:
        raise RuntimeError(result.stderr)
    return json.loads(result.stdout) if action != "save" else None


def load(path):
    return call("load", path)


def save(path, value):
    return call("save", path, value)
