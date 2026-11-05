"""Small queue bridge for the retired Python Brain compatibility commands."""

from contextlib import contextmanager
import fcntl
import json
import os
from pathlib import Path
import stat
import tempfile
import threading
import time

_held = threading.local()


@contextmanager
def vault_lock(root):
    root = Path(root).resolve()
    state = root / ".brain-state"
    if state.is_symlink():
        raise ValueError("Symlinked Brain state directory")
    state.mkdir(parents=True, exist_ok=True, mode=0o700)
    if not state.is_dir() or state.is_symlink():
        raise ValueError("Invalid Brain state directory")
    held = getattr(_held, "roots", {})
    key = str(root)
    if held.get(key):
        held[key] += 1
        _held.roots = held
        try:
            yield
        finally:
            held[key] -= 1
        return
    locks = []
    try:
        for name in ("lock", "capture.lock"):
            path = state / name
            fd = os.open(
                path,
                os.O_CREAT | os.O_RDWR | os.O_APPEND | os.O_NOFOLLOW,
                0o600,
            )
            info = os.fstat(fd)
            if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
                os.close(fd)
                raise ValueError("Linked or non-regular Brain lock")
            os.fchmod(fd, 0o600)
            stream = os.fdopen(fd, "a")
            fcntl.flock(stream, fcntl.LOCK_EX)
            locks.append(stream)
        held[key] = 1
        _held.roots = held
        yield
    finally:
        if locks:
            held.pop(key, None)
            _held.roots = held
        for stream in reversed(locks):
            fcntl.flock(stream, fcntl.LOCK_UN)
            stream.close()


def enqueue_locked(root, path, agent="python-compat"):
    root = Path(root)
    relative = Path(path).relative_to(root).as_posix()
    if relative == ".brain-state" or relative.startswith(".brain-state/"):
        return
    if relative == "credentials" or relative.startswith("credentials/"):
        return
    state = root / ".brain-state"
    if state.is_symlink():
        raise ValueError("Symlinked Brain state directory")
    state.mkdir(parents=True, exist_ok=True, mode=0o700)
    queue = state / "git-queue.json"
    try:
        queue_info = queue.lstat()
    except FileNotFoundError:
        queue_info = None
    if queue_info and (
        not stat.S_ISREG(queue_info.st_mode) or queue_info.st_nlink != 1
    ):
        raise ValueError("Invalid Brain git queue file")
    try:
        contents = queue.read_text()
    except FileNotFoundError:
        value = {"entries": []}
    else:
        try:
            value = json.loads(contents)
        except json.JSONDecodeError:
            raise ValueError("Invalid Brain git queue") from None
    entries = value.setdefault("entries", [])
    existing = next((item for item in entries if item.get("path") == relative), None)
    if existing:
        existing["agent"] = agent
    else:
        entries.append({"path": relative, "agent": agent})
    value["updated"] = int(time.time())
    fd, temporary = tempfile.mkstemp(prefix="git-queue-", suffix=".tmp", dir=state)
    try:
        os.fchmod(fd, 0o600)
        with os.fdopen(fd, "w") as stream:
            json.dump(value, stream, indent=2)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, queue)
    finally:
        Path(temporary).unlink(missing_ok=True)


def enqueue(root, path, agent="python-compat"):
    key = str(Path(root).resolve())
    if getattr(_held, "roots", {}).get(key):
        enqueue_locked(root, path, agent)
    else:
        with vault_lock(root):
            enqueue_locked(root, path, agent)


@contextmanager
def capture_claim(root, signature):
    """Serialize identical compatibility captures without blocking native writes."""
    state = Path(root).resolve() / ".brain-state"
    if state.is_symlink():
        raise ValueError("Symlinked Brain state directory")
    state.mkdir(parents=True, exist_ok=True, mode=0o700)
    path = state / ("capture-" + signature + ".lock")
    fd = os.open(
        path, os.O_CREAT | os.O_RDWR | os.O_APPEND | os.O_NOFOLLOW, 0o600
    )
    info = os.fstat(fd)
    if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
        os.close(fd)
        raise ValueError("Linked or non-regular Brain capture claim")
    os.fchmod(fd, 0o600)
    stream = os.fdopen(fd, "a")
    try:
        fcntl.flock(stream, fcntl.LOCK_EX)
        yield
    finally:
        fcntl.flock(stream, fcntl.LOCK_UN)
        stream.close()
