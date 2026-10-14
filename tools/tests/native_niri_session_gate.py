"""Developer-only native argument mapping for historical acceptance tests."""

import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def invoke(request):
    binary = os.environ.get("NACRE_SESSION_GATE_PARITY") or str(
        ROOT / "target/debug/examples/niri-session-gate-parity"
    )
    result = subprocess.run(
        [binary, json.dumps(request)], capture_output=True, text=True, check=True
    )
    return json.loads(result.stdout)


def allowed(action, environment=None, home=None):
    return invoke(
        {
            "operation": "allowed",
            "action": action,
            "environment": dict(os.environ if environment is None else environment),
            "home": str(Path.home() if home is None else home),
        }
    )


def compositor(environment):
    return invoke({"operation": "compositor", "environment": environment})
