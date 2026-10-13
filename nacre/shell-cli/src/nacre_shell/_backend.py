"""Pinned persistent transport to Orient's native binary; no engine implementation."""

import atexit
import json
import os
from pathlib import Path
import shutil
import select
import signal
import subprocess
import sys
import threading
import time

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


TIMEOUT = 120
LIMIT = 16 * 1024 * 1024
_process = None
_lock = threading.Lock()
_next_id = 0
_verified = False


def _close(cancel=False):
    global _process, _verified
    process, _process = _process, None
    _verified = False
    if process is None:
        return
    try:
        process.stdin.close()
        if cancel and process.poll() is None:
            process.send_signal(signal.SIGINT) if os.name == "posix" else process.terminate()
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
    finally:
        process.stdout.close()


def _after_fork():
    # Inherited pipe descriptors belong to the parent; never signal its child.
    global _process, _verified, _lock
    if _process is not None:
        _process.stdin.close()
        _process.stdout.close()
    _process = None
    _verified = False
    _lock = threading.Lock()


if hasattr(os, "register_at_fork"):
    os.register_at_fork(after_in_child=_after_fork)
atexit.register(_close)


def _exchange(request, deadline):
    process = _process
    incoming = bytearray()
    outgoing = memoryview(request)
    while True:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise subprocess.TimeoutExpired(process.args, TIMEOUT)
        readable, writable, _ = select.select(
            [] if outgoing else [process.stdout], [process.stdin] if outgoing else [], [], remaining
        )
        if writable:
            outgoing = outgoing[os.write(process.stdin.fileno(), outgoing[:65536]) :]
        if readable:
            chunk = os.read(process.stdout.fileno(), 65536)
            if not chunk:
                raise OSError("Native Orient exited before replying")
            incoming.extend(chunk)
            if b"\n" in chunk:
                if not incoming.endswith(b"\n") or incoming.count(b"\n") != 1:
                    raise OSError("Native Orient returned an invalid reply frame")
                return json.loads(incoming)


def call(operation, **payload):
    global _process, _next_id, _verified
    deadline = time.monotonic() + TIMEOUT
    if not _lock.acquire(timeout=TIMEOUT):
        raise subprocess.TimeoutExpired(["orient", "--rpc", "--serve"], TIMEOUT)
    try:
        if _process is not None and _process.poll() is not None:
            _close()
        if _process is None:
            _process = subprocess.Popen(
                [str(binary()), "--rpc", "--serve"],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
                bufsize=0,
            )
            os.set_blocking(_process.stdin.fileno(), False)
            os.set_blocking(_process.stdout.fileno(), False)
        _next_id += 1
        request_id = _next_id
        request = (json.dumps({"id": request_id, "operation": operation, "payload": payload}) + "\n").encode()
        if len(request) > LIMIT:
            raise ValueError("Native request exceeds its 16 MiB limit")
        try:
            reply = _exchange(request, deadline)
            if not isinstance(reply, dict) or reply.get("id") != request_id:
                raise OSError("Native Orient returned an invalid reply")
            if reply.get("interfaceVersion") != 1:
                raise OSError("Native Orient differs from Nacre's tested pin; provision the palette runtime")
            if not _verified:
                if reply.get("engine") != ENGINE_ID:
                    raise OSError("Native Orient differs from Nacre's tested pin; provision the palette runtime")
                _verified = True
            if "result" not in reply and "error" not in reply:
                raise OSError("Native Orient returned an invalid reply")
        except BaseException:
            _close(cancel=True)
            raise
        failure = reply.get("error")
        if failure:
            kind = {"key": KeyError, "value": ValueError, "io": OSError}.get(failure.get("kind"), OSError)
            raise kind(failure.get("message", "Native Orient failed"))
        return reply["result"]
    finally:
        _lock.release()
