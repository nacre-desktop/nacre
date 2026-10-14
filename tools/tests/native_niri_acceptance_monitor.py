"""Developer data mapping; acceptance decisions and hashes stay in Rust."""

import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def invoke(request):
    binary = os.environ.get("NACRE_ACCEPTANCE_PARITY") or str(
        ROOT / "target/debug/examples/niri-acceptance-parity"
    )
    result = subprocess.run(
        [binary, json.dumps(request)],
        capture_output=True,
        text=True,
        timeout=10,
        check=True,
    )
    return json.loads(result.stdout)


class Cycle:
    def __init__(self):
        self.samples = []

    def observe(self, sample):
        self.samples.append(sample)
        return invoke({"operation": "cycle", "samples": self.samples})[-1]


def protected_files_match(backup, home):
    return invoke(
        {"operation": "protected_files_match", "backup": str(backup), "home": str(home)}
    )
