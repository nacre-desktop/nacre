"""Temporary native-only client for Python callers retired in later waves.

No compositor encoding or state reduction lives here. Calls share one native
JSON-lines worker; failures never replay an action automatically.
"""

import atexit
import builtins
import json
import os
from pathlib import Path
import select
import subprocess
import sys
import threading

_worker = None
_sequence = 0
_lock = threading.Lock()
_missing = object()
_capabilities = None
_environment = None


def select_backend(environment=None):
    environment = os.environ if environment is None else environment
    explicit = environment.get("NACRE_COMPOSITOR", "").lower()
    if explicit:
        if explicit not in ("hyprland", "niri"):
            raise ValueError("Unknown NACRE_COMPOSITOR backend")
        return explicit
    if "niri" in environment.get("XDG_CURRENT_DESKTOP", "").lower().split(":"):
        return "niri"
    if environment.get("HYPRLAND_INSTANCE_SIGNATURE"):
        return "hyprland"
    return "niri" if environment.get("NIRI_SOCKET") else "hyprland"


backend_name = select_backend()
backend_display_name = "Hyprland" if backend_name == "hyprland" else "niri"
night_light_tool = "hyprsunset" if backend_name == "hyprland" else "wlsunset"
night_light_service = (
    "hyprsunset.service" if backend_name == "hyprland" else "nacre-night-light.service"
)
portal_service = (
    "xdg-desktop-portal-hyprland.service"
    if backend_name == "hyprland"
    else "xdg-desktop-portal-gnome.service"
)


def _close():
    global _worker
    worker, _worker = _worker, None
    if worker is None:
        return
    if worker.stdin:
        worker.stdin.close()
    try:
        worker.wait(timeout=2)
    except subprocess.TimeoutExpired:
        worker.kill()
        worker.wait()
    if worker.stdout:
        worker.stdout.close()


atexit.register(_close)


def _encode(value):
    if isinstance(value, Path):
        return str(value)
    if isinstance(value, dict):
        return {key: _encode(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_encode(item) for item in value]
    return value


def _binary():
    override = os.environ.get("NACRE_HELPER_BINARY")
    if override:
        return Path(override)
    directory = Path(__file__).resolve().parent
    root = directory.parents[1]
    if (root / "Cargo.toml").is_file():
        candidates = [
            root / "target" / mode / "nacre-helper" for mode in ("release", "debug")
        ]
        available = [path for path in candidates if path.is_file()]
        if available:
            return max(available, key=lambda path: path.stat().st_mtime_ns)
    return directory / "nacre-helper"


def _call(operation, *args, **kwargs):
    global _worker, _sequence, _environment
    with _lock:
        environment = {
            name: os.environ.get(name, "")
            for name in (
                "HOME",
                "XDG_CONFIG_HOME",
                "XDG_STATE_HOME",
                "XDG_CACHE_HOME",
                "NACRE_COMPOSITOR",
                "XDG_CURRENT_DESKTOP",
                "HYPRLAND_INSTANCE_SIGNATURE",
                "NIRI_SOCKET",
                "NIRI_CONFIG",
                "XDG_RUNTIME_DIR",
                "WAYLAND_DISPLAY",
                "NACRE_NIRI_CONFIG",
                "NACRE_NIRI_DESKTOP_CONFIG",
            )
        }
        if _environment != environment:
            _close()
            _environment = environment
        if _worker is None or _worker.poll() is not None:
            _close()
            _worker = subprocess.Popen(
                [str(_binary()), "compositor", "serve-api"],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                # Native diagnostics reach the existing journal/terminal.
                stderr=None,
                text=True,
                bufsize=1,
            )
        _sequence += 1
        request = {
            "id": _sequence,
            "operation": operation,
            "args": _encode(args),
            "kwargs": _encode(kwargs),
        }
        try:
            _worker.stdin.write(json.dumps(request, allow_nan=False) + "\n")
            _worker.stdin.flush()
            timeout = kwargs.get("timeout", 10)
            timeout = 30 if timeout is None else max(2, timeout * 12 + 2)
            if not select.select([_worker.stdout], [], [], timeout)[0]:
                raise TimeoutError("Native compositor worker timed out")
            line = _worker.stdout.readline()
            if not line:
                raise RuntimeError("Native compositor worker disconnected")
            response = json.loads(line)
            if response.get("id") != _sequence:
                raise RuntimeError("Unexpected native compositor response ID")
        except (OSError, ValueError, RuntimeError):
            _close()
            raise
        if "error" in response:
            kind = response.get("type", "RuntimeError")
            exception = getattr(builtins, kind, RuntimeError)
            raise exception(response["error"])
        value = response["value"]
        if isinstance(value, dict) and set(value) == {"code", "stdout", "stderr"}:
            return subprocess.CompletedProcess(
                [], value["code"], value["stdout"], value["stderr"]
            )
        if operation in ("config_path", "palette_template_path"):
            return Path(value)
        if operation == "binding_source_files":
            return [Path(item) for item in value]
        return value


def query(name, *, timeout=10, default=_missing, max_bytes=None):
    if name not in (
        "monitors",
        "all_monitors",
        "workspaces",
        "windows",
        "active_window",
        "active_workspace",
        "cursor",
        "instances",
        "bindings",
    ):
        raise ValueError("Unknown compositor query")
    try:
        return _call("query", name, timeout=timeout, max_bytes=max_bytes)
    except (OSError, ValueError, RuntimeError) as error:
        if default is _missing:
            raise
        print("nacre-helper: compositor query fallback: " + str(error), file=sys.stderr)
        return default


def config_errors(*, timeout=10, default=_missing):
    try:
        return _call("config_errors", timeout=timeout)
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        if default is _missing:
            raise
        print(
            "nacre-helper: compositor config fallback: " + str(error), file=sys.stderr
        )
        return default


def session_context(environment):
    return _call(
        "session_context",
        {
            key: environment[key]
            for key in (
                "XDG_CURRENT_DESKTOP",
                "NIRI_SOCKET",
                "NIRI_CONFIG",
                "HYPRLAND_INSTANCE_SIGNATURE",
            )
            if key in environment
        },
    )


def render_for_backend(backend, operation, values):
    result = subprocess.run(
        [
            str(_binary()),
            "compositor-render",
            backend,
            operation,
            json.dumps(_encode(values), allow_nan=False),
        ],
        capture_output=True,
        text=True,
        timeout=10,
    )
    if result.stderr:
        print(result.stderr, end="", file=sys.stderr)
    value = json.loads(result.stdout)
    if result.returncode:
        raise getattr(builtins, value.get("type", "RuntimeError"), RuntimeError)(
            value.get("error", result.stderr)
        )
    return value


def __getattr__(operation):
    global _capabilities
    if operation.startswith("__"):
        raise AttributeError(operation)
    if operation == "capabilities":
        if _capabilities is None:
            _capabilities = _call(operation)
        return _capabilities
    return lambda *args, **kwargs: _call(operation, *args, **kwargs)
