"""Developer-only native call mapping for the retired wrapper assertions."""

import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]


def main(argv=None, service=None):
    argv = sys.argv[1:] if argv is None else argv
    binary = Path(
        os.environ.get("NACRE_COMPAT_PARITY")
        or ROOT / "target/debug/examples/compositor-compat-parity"
    )
    result = subprocess.run(
        [str(binary), *argv], capture_output=True, text=True, check=True
    )
    response = json.loads(result.stdout)
    if "error" in response:
        raise ValueError(response["error"])
    call = response["call"]
    value = getattr(service, call["method"])(*call["args"], **call["kwargs"])
    if argv[0] == "valid-window-id":
        return 0 if value else 1
    if argv[0] == "window-job-token":
        print(value)
        return 0
    if argv[0] == "query":
        print(json.dumps(value))
    return None
