"""Pinned one-shot transport to Orient's native binary; no engine implementation."""

import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys

PIN = json.loads(Path(__file__).with_name("engine-pin.json").read_text())
ENGINE_ID = PIN["engine"]
FORMAT_VERSION = 1
__version__ = PIN["version"].replace("-dev.", ".dev")


def binary():
    explicit = os.environ.get("NACRE_ORIENT_BINARY")
    if explicit:
        return Path(explicit).expanduser().resolve(strict=True)
    candidates = [Path(sys.prefix) / "bin/orient", Path.home() / ".local/share/nacre/palette-runtime/venv/bin/orient"]
    system = shutil.which("orient")
    if system:
        candidates.append(Path(system))
    for path in candidates:
        if path.is_file():
            return path
    raise OSError("Native Orient is missing; provision the pinned palette runtime")


def call(operation, **payload):
    command = [str(binary()), "--rpc"]
    process = subprocess.Popen(
        command,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        start_new_session=os.name == "posix",
    )
    try:
        output, error = process.communicate(json.dumps({"operation": operation, "payload": payload}), timeout=120)
    except BaseException:
        if process.poll() is None:
            if os.name == "posix":
                os.killpg(process.pid, signal.SIGINT)
            else:
                process.terminate()
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        raise
    try:
        reply = json.loads(output)
    except ValueError as failure:
        raise OSError("Native Orient returned an invalid reply: " + error[-300:]) from failure
    if reply.get("interfaceVersion") != 1 or reply.get("engine") != ENGINE_ID:
        raise OSError("Native Orient differs from Nacre's tested pin; provision the palette runtime")
    failure = reply.get("error")
    if failure:
        kind = {"key": KeyError, "value": ValueError, "io": OSError}.get(failure.get("kind"), OSError)
        raise kind(failure.get("message", "Native Orient failed"))
    if process.returncode:
        raise OSError("Native Orient failed: " + error[-300:])
    return reply["result"]
