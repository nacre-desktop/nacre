"""Private XDG paths and atomic cache/state files; no theme publisher here."""

from contextlib import contextmanager
import fcntl
import hashlib
import json
import os
from pathlib import Path
import tempfile

from . import ENGINE_ID


def roots():
    home = Path.home()
    return (
        Path(os.environ.get("XDG_CONFIG_HOME", str(home / ".config"))) / "nacre",
        Path(os.environ.get("XDG_STATE_HOME", str(home / ".local/state"))) / "nacre",
        Path(os.environ.get("XDG_CACHE_HOME", str(home / ".cache"))) / "nacre",
    )


def read(path, default):
    try:
        return json.loads(path.read_text())
    except FileNotFoundError:
        return default


def atomic(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(prefix="." + path.name, dir=path.parent)
    try:
        with os.fdopen(fd, "w") as stream:
            stream.write(text)
        os.replace(name, path)
    finally:
        Path(name).unlink(missing_ok=True)


def write_json(path, data):
    atomic(path, json.dumps(data, sort_keys=True) + "\n")


def link(path, target):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(dir=path.parent)
    os.close(fd)
    Path(name).unlink()
    try:
        Path(name).symlink_to(target)
        os.replace(name, path)
    finally:
        Path(name).unlink(missing_ok=True)


@contextmanager
def commit_lock():
    if os.environ.get("NACRE_PALETTE_LOCKED") == "1":
        yield
        return
    _, state, _ = roots()
    state.mkdir(parents=True, exist_ok=True)
    with (state / "palette-commit.lock").open("a") as stream:
        fcntl.flock(stream, fcntl.LOCK_EX)
        yield


def cache_key(path, settings):
    path = Path(path).resolve(strict=True)
    stat = path.stat()
    identity = (str(path), stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns)
    return hashlib.sha256(json.dumps((ENGINE_ID, identity, settings), sort_keys=True).encode()).hexdigest()
