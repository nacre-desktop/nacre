"""Developer provider mapping preserving the historical runtime assertions."""

import json
import math
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def wire(value):
    # JSON has no NaN/Infinity. Null retains the validator's rejection branch;
    # this is an invalid input provider mapping, never an expected-output replay.
    if isinstance(value, float) and not math.isfinite(value):
        return None
    if isinstance(value, dict):
        return {key: wire(item) for key, item in value.items()}
    if isinstance(value, list):
        return [wire(item) for item in value]
    return value


def invoke(request):
    binary = os.environ.get("NACRE_SESSION_RUNTIME_PARITY") or str(
        ROOT / "target/debug/examples/niri-session-runtime-parity"
    )
    result = subprocess.check_output(
        [binary, json.dumps(wire(request), allow_nan=False)], text=True, timeout=10
    )
    message = json.loads(result)
    if "error" in message:
        raise ValueError(message["error"])
    return message["result"]


def command(action, environment=None, home=None):
    argv, environment = invoke(
        {
            "operation": "command",
            "action": action,
            "environment": dict(os.environ if environment is None else environment),
            "home": str(Path.home() if home is None else home),
        }
    )
    return argv, environment


def host_preferences(desktop):
    return invoke({"operation": "host_preferences", "desktop": desktop})


def palette_include(home, config):
    return Path(
        invoke(
            {"operation": "palette_include", "home": str(home), "config": str(config)}
        )
    )
