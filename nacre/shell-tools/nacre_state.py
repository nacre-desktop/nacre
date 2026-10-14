"""Versioned Nacre-owned JSON, lazy migration and private original-byte backups.

Readers accept legacy data without modifying it. The next successful save migrates
under a cross-process lock; newer versions raise before either reads or writes.
"""

from contextlib import contextmanager
import fcntl
from fnmatch import fnmatchcase
import json
import os
from pathlib import Path
import tempfile

SCHEMAS = json.loads(
    Path(__file__).resolve().with_name("nacre-state-files.json").read_text()
)["files"]
LIMIT = 16 * 1024 * 1024


class InvalidVersionError(RuntimeError):
    """Unsupported version metadata is protected, not corrupt payload recovery."""


class NewerVersionError(RuntimeError):
    """A downgrade must not fall back to defaults and overwrite this file."""


ORDERED_SCHEMAS = sorted(
    SCHEMAS, key=lambda item: max(map(len, item["paths"])), reverse=True
)


def schema(path):
    name = Path(path).as_posix()
    for entry in ORDERED_SCHEMAS:
        if any(fnmatchcase(name, "*/" + pattern) for pattern in entry["paths"]):
            return entry
    return None


def _regular(path):
    if path.is_symlink():
        raise RuntimeError("Refusing linked state file")
    if path.exists() and not path.is_file():
        raise RuntimeError("State path is not a regular file")


@contextmanager
def _lock(path):
    path.parent.mkdir(parents=True, exist_ok=True)
    lock = path.with_name(path.name + ".schema.lock")
    fd = os.open(lock, os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    try:
        fcntl.flock(fd, fcntl.LOCK_EX)
        _regular(path)
        yield
    finally:
        os.close(fd)


def _publish(path, data, mode=0o600):
    fd, name = tempfile.mkstemp(prefix="." + path.name + ".", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.chmod(name, mode)
        os.replace(name, path)
        directory = os.open(path.parent, os.O_DIRECTORY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        Path(name).unlink(missing_ok=True)


def _backup(path, data, label):
    backup = path.with_name(path.name + label + ".bak")
    _regular(backup)
    # Publish a complete backup without ever replacing an earlier original.
    fd, name = tempfile.mkstemp(prefix=".state-backup-", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        try:
            os.link(name, backup)
        except FileExistsError:
            pass
        directory = os.open(path.parent, os.O_DIRECTORY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        Path(name).unlink(missing_ok=True)


def _decode(value, entry):
    version = value.get("version", 0) if isinstance(value, dict) else 0
    if entry.get("keepVersion") and (
        version is True or type(version) is float and version == 1.0
    ):
        version = 0  # Legacy permissive version metadata is canonicalized with backup.
    if type(version) is not int or version < 0:
        raise InvalidVersionError("Unsupported state version; original file preserved")
    if version > entry["version"]:
        raise NewerVersionError("Newer state version; original file preserved")
    if entry["shape"] == "array":
        data = value.get("items") if isinstance(value, dict) and version else value
        if not isinstance(data, list):
            raise ValueError("Invalid state list")
    else:
        if not isinstance(value, dict):
            raise ValueError("Invalid state object")
        data = dict(value)
        if not entry.get("keepVersion"):
            data.pop("version", None)
    if (
        entry.get("keepVersion")
        and isinstance(data, dict)
        and version < entry["version"]
    ):
        data["version"] = entry["version"]
    if entry["id"] == "control-center" and version < 2:
        data = dict(
            data,
            version=2,
            tiles=data.get(
                "tiles", entry["defaults"]["tiles"] + data.get("optionalTiles", [])
            ),
        )
        data.pop("optionalTiles", None)
    return data, version


def _encode(data, entry):
    if entry["shape"] == "array":
        if not isinstance(data, list):
            raise ValueError("Invalid state list")
        value = {"version": entry["version"], "items": data}
    else:
        if not isinstance(data, dict):
            raise ValueError("Invalid state object")
        value = dict(data, version=entry["version"])
    return (json.dumps(value, ensure_ascii=True) + "\n").encode()


def _bytes(path):
    with path.open("rb") as stream:
        data = stream.read(LIMIT + 1)
    if len(data) > LIMIT:
        raise RuntimeError("State file too large; original preserved")
    return data


def read(path, default=None, *, schema_id=None):
    path = Path(path)
    entry = (
        next(item for item in SCHEMAS if item["id"] == schema_id)
        if schema_id
        else schema(path)
    )
    _regular(path)
    try:
        raw = _bytes(path)
    except FileNotFoundError:
        return default
    if entry is None:
        return json.loads(raw)
    try:
        return _decode(json.loads(raw), entry)[0]
    except (ValueError, UnicodeError):
        # Recovery is on save; reads may return defaults but keep corrupt bytes.
        return default


def write(path, data, *, mode=0o600, schema_id=None, _serialized=None, _recover=False):
    path = Path(path)
    entry = (
        next(item for item in SCHEMAS if item["id"] == schema_id)
        if schema_id
        else schema(path)
    )
    if entry is None:
        path.parent.mkdir(parents=True, exist_ok=True)
        _regular(path)
        _publish(path, (json.dumps(data) + "\n").encode(), mode)
        return
    # Validate the new value before backing up or touching existing state.
    if (
        isinstance(data, dict)
        and type(data.get("version", 0)) is int
        and data.get("version", 0) > entry["version"]
    ):
        raise NewerVersionError("Cannot write a newer state version")
    encoded = _encode(data, entry)
    if _serialized is not None:
        encoded = _serialized
    if len(encoded) > LIMIT:
        raise RuntimeError("State file too large; original preserved")
    with _lock(path):
        try:
            raw = _bytes(path)
        except FileNotFoundError:
            raw = None
        if raw is not None:
            try:
                _, old = _decode(json.loads(raw), entry)
            except (ValueError, UnicodeError):
                _backup(path, raw, ".corrupt")
            else:
                if _recover:
                    _backup(path, raw, ".corrupt")
                elif old < entry["version"]:
                    _backup(path, raw, f".pre-v{old}-to-v{entry['version']}")
        _publish(path, encoded, mode)


def write_text_if_managed(path, text, *, mode=0o600):
    """Let existing mixed text/JSON writers retain their non-state contracts."""
    if schema(path) is None:
        return False
    value = json.loads(text)
    entry = schema(path)
    current = (
        isinstance(value, dict)
        and type(value.get("version")) is int
        and value.get("version") == entry["version"]
    )
    serialized = text.encode() if isinstance(text, str) else text
    if entry["shape"] == "array" and current:
        value = value["items"]
    write(path, value, mode=mode, _serialized=serialized if current else None)
    return True


def read_json_file(path, schema_id=None):
    """Strict missing-file behavior, with schema defaults for corrupt owned state."""
    path = Path(path)
    if not path.exists():
        raise FileNotFoundError(path)
    entry = (
        next(item for item in SCHEMAS if item["id"] == schema_id)
        if schema_id
        else schema(path)
    )
    fallback = json.loads(json.dumps(entry["defaults"])) if entry else None
    return read(path, fallback, schema_id=schema_id)


def recover(path, data):
    """Recover a caller-validated bad payload, retaining bytes and downgrade checks."""
    write(path, data, _recover=True)
