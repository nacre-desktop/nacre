"""Developer-only input provider for the unchanged readiness assertions."""

import json
import os
from pathlib import Path
import subprocess

REPO = Path(__file__).resolve().parents[3]
STATE = Path.home() / ".local/state/nacre/session"


def run(args, *rest, **kwargs):
    return ""


def check(event="manual"):
    binary = os.environ.get(
        "NACRE_SESSION_PARITY", str(REPO / "target/debug/examples/session-parity")
    )
    payload = dict(
        event=event,
        state=str(STATE),
        desktop=run(["qs"]),
        wallet=run(["busctl"]),
    )
    return json.loads(
        subprocess.check_output([binary], input=json.dumps(payload), text=True)
    )
