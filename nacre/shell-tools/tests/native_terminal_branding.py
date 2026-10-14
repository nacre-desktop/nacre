"""Developer-only identity fixture adapter for the original PTY assertions."""

import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]


def invoke(request):
    binary = os.environ.get("NACRE_TERMINAL_PARITY") or str(
        ROOT / "target/debug/examples/terminal-branding-parity"
    )
    result = subprocess.run(
        [binary, json.dumps(request)],
        capture_output=True,
        text=True,
        check=True,
        timeout=10,
    )
    return json.loads(result.stdout)


def identity(pid, proc=Path("/proc")):
    return tuple(invoke({"operation": "identity", "pid": int(pid), "proc": str(proc)}))


def refresh(home, proc=Path("/proc")):
    records = []
    cache = Path(home) / ".cache/nacre/terminal-logos"
    for file in cache.glob("*.json"):
        record = json.loads(file.read_text())
        if "pid" in record:
            records.append((int(record["pid"]), identity(int(record["pid"]), proc)))
    if not records:
        invoke({"operation": "refresh", "home": str(home), "proc": str(proc)})
        return
    with tempfile.TemporaryDirectory(prefix="terminal-identity-fixture-") as folder:
        for pid, (parent, device, started) in records:
            target = Path(folder) / str(pid)
            target.mkdir(exist_ok=True)
            fields = ["0"] * 25
            fields[0], fields[1], fields[4], fields[19] = "S", parent, device, started
            (target / "stat").write_text(f"{pid} (fixture) " + " ".join(fields))
        invoke({"operation": "refresh", "home": str(home), "proc": folder})
