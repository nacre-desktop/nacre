"""Developer-only facade for retained Python recovery assertions."""

import json
import os
from pathlib import Path
import subprocess

REPO = Path(__file__).resolve().parents[3]
ROOT = Path.home() / ".local/share/nacre/shell"
GOOD = ROOT / "source.good"


def _call(*args):
    binary = Path(
        os.environ.get(
            "NACRE_SUPERVISOR_PARITY", REPO / "target/debug/examples/supervisor-parity"
        )
    )
    result = subprocess.run(
        [str(binary), *args], text=True, capture_output=True, check=True
    )
    lines = result.stdout.splitlines()
    for line in lines[:-1]:
        print(line, flush=True)
    return json.loads(lines[-1])


def restore():
    return _call("restore", str(ROOT))


def health(env):
    return _call("health-module", str(ROOT), json.dumps(env))
