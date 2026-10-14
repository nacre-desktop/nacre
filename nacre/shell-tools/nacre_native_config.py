"""Temporary native compiler client for callers retired in later Rust waves.

No configuration lowering lives here. The installed helper is adjacent to this
file; source and developer checks can select an explicit compiled candidate.
"""

import ast
import builtins
from functools import lru_cache
import json
import os
from pathlib import Path
import subprocess

DIRECTORY = Path(__file__).resolve().parent


def _encode(value):
    if isinstance(value, Path):
        return {"$path": str(value)}
    if isinstance(value, dict):
        if value and all(type(key) is int for key in value):
            return {
                "$integer_map": [[key, _encode(item)] for key, item in value.items()]
            }
        return {key: _encode(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_encode(item) for item in value]
    return value


def _binary():
    override = os.environ.get("NACRE_HELPER_BINARY")
    if override:
        return Path(override)
    installed = DIRECTORY / "nacre-helper"
    if installed.is_file():
        return installed
    root = DIRECTORY.parents[1]
    candidates = [
        root / "target" / mode / "nacre-helper" for mode in ("release", "debug")
    ]
    candidates = [path for path in candidates if path.is_file()]
    if not candidates:
        raise RuntimeError("Build the native nacre-helper before using the compiler")
    return max(candidates, key=lambda path: path.stat().st_mtime_ns)


def _call(module, operation, *args, **kwargs):
    if operation in ("load", "render"):
        index = 0 if operation == "load" else 1
        if len(args) <= index:
            kwargs.setdefault("directory", DIRECTORY)
    if operation == "render_profile":
        kwargs.setdefault("directory", DIRECTORY)
    request = json.dumps({"args": _encode(args), "kwargs": _encode(kwargs)})
    result = subprocess.run(
        [str(_binary()), module, operation, request],
        stdin=subprocess.DEVNULL,
        capture_output=True,
        text=True,
        timeout=10,
        check=False,
    )
    if result.stderr or result.returncode not in (0, 1):
        raise RuntimeError("Native compiler failed: " + result.stderr.strip())
    value = json.loads(result.stdout)
    if result.returncode:
        exception = getattr(builtins, value["type"], ValueError)
        message = value["error"]
        if exception is KeyError:
            message = ast.literal_eval(message)
        raise exception(message)
    return tuple(value) if operation == "supported_backends" else value


@lru_cache(maxsize=2)
def _outputs(backend):
    return tuple(
        _call("compositor_config", "backend_capabilities", backend)["generatedFiles"]
    )


class _Backend:
    def __init__(self, backend):
        self.backend = backend

    def __getattr__(self, operation):
        if operation.startswith("__"):
            raise AttributeError(operation)
        if operation == "OUTPUTS":
            return _outputs(self.backend)
        return lambda *args, **kwargs: _call(
            self.backend + "_config", operation, *args, **kwargs
        )


hyprland = _Backend("hyprland")
niri = _Backend("niri")


def __getattr__(operation):
    if operation.startswith("__"):
        raise AttributeError(operation)
    if operation == "OUTPUTS":
        return _outputs("hyprland")
    return lambda *args, **kwargs: _call(
        "compositor_config", operation, *args, **kwargs
    )
