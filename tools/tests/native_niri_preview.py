"""Developer-only data mapping preserving the original preview assertions."""

import json
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def invoke(request):
    binary = os.environ.get("NACRE_PREVIEW_PARITY") or str(
        ROOT / "target/debug/examples/niri-preview-parity"
    )
    process = subprocess.run(
        [binary, json.dumps(request)],
        capture_output=True,
        text=True,
        timeout=10,
        check=True,
    )
    response = json.loads(process.stdout)
    if "error" in response:
        raise ValueError(response["error"])
    return response["result"]


allowlists = invoke({"operation": "allowlists"})
SAFE_CONFIG = tuple(allowlists["config"])
SAFE_STATE = tuple(allowlists["state"])


def command(destination, home, environment, runtime):
    return invoke(
        {
            "operation": "command",
            "destination": str(destination),
            "home": str(home),
            "environment": environment,
            "runtime": str(runtime),
            "quickshell": shutil.which("quickshell"),
        }
    )


def private_copy(source, target):
    return invoke(
        {"operation": "private_copy", "source": str(source), "target": str(target)}
    )


def copy_cached_presentation(home, state, cache):
    return invoke(
        {
            "operation": "copy_cached_presentation",
            "home": str(home),
            "state": str(state),
            "cache": str(cache),
        }
    )


def preview_bindings(home, names):
    return invoke({"operation": "preview_bindings", "home": str(home), "names": names})
