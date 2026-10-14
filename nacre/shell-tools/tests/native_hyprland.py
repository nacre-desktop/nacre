"""Developer-only callback facade retaining original Hyprland assertion bodies."""

import builtins
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
BINARY = Path(
    os.environ.get("NACRE_HYPRLAND_PARITY")
    or ROOT / "target/debug/examples/hyprland-parity"
)


class Backend:
    def _run(self, args, *, timeout=10, check=True):
        raise RuntimeError("Test must provide a native transport")

    def query_result(self, name, *, timeout=10, check=True):
        commands = {
            "monitors": ["monitors", "-j"],
            "all_monitors": ["monitors", "all", "-j"],
            "workspaces": ["workspaces", "-j"],
            "windows": ["clients", "-j"],
            "active_window": ["activewindow", "-j"],
            "active_workspace": ["activeworkspace", "-j"],
            "cursor": ["cursorpos", "-j"],
            "instances": ["instances", "-j"],
            "bindings": ["binds", "-j"],
        }
        if name not in commands:
            raise ValueError("Unknown compositor query")
        return self._run(commands[name], timeout=timeout, check=check)

    def invoke(self, operation, args, kwargs, **extra):
        request = {
            "operation": operation,
            "args": args,
            "kwargs": kwargs,
            "observe_nested": operation in ("set_animations", "set_performance")
            and ("set_options" in vars(self) or "reload" in vars(self)),
            **extra,
        }
        with subprocess.Popen(
            [str(BINARY), json.dumps(request)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        ) as process:
            try:
                while line := process.stdout.readline():
                    message = json.loads(line)
                    if message.get("callback") == "nested":
                        getattr(self, message["method"])(
                            *message["args"], timeout=message["timeout"]
                        )
                        process.stdin.write("null\n")
                        process.stdin.flush()
                    elif message.get("callback") == "run":
                        result = self._run(
                            message["args"],
                            timeout=message["timeout"],
                            check=message["check"],
                        )
                        value = {
                            "code": result.returncode,
                            "stdout": result.stdout,
                            "stderr": result.stderr or "",
                        }
                        process.stdin.write(json.dumps(value) + "\n")
                        process.stdin.flush()
                    else:
                        result = message["result"]
                        if "error" in result:
                            raise getattr(builtins, result["type"], RuntimeError)(
                                result["error"]
                            )
                        value = result["value"]
                        if isinstance(value, dict) and set(value) == {
                            "code",
                            "stdout",
                            "stderr",
                        }:
                            return subprocess.CompletedProcess(
                                [], value["code"], value["stdout"], value["stderr"]
                            )
                        return value
                raise RuntimeError(process.stderr.read())
            finally:
                process.stdin.close()
                process.wait(timeout=5)

    def query(self, name, *, timeout=10, max_bytes=None):
        # Legacy query_result is the injected provider. Rust parses and normalizes.
        result = self.query_result(name, timeout=timeout)
        return self.invoke(
            "normalize_reply", [name], {"max_bytes": max_bytes}, stdout=result.stdout
        )

    def session_id(self):
        return self.invoke("session_id", [], {}, environment=dict(os.environ))

    def environment_updates(self, environment=None):
        result = self.query_result("instances", timeout=None, check=False)
        return self.invoke(
            "environment_reply",
            [dict(os.environ) if environment is None else environment],
            {},
            reply={
                "code": result.returncode,
                "stdout": result.stdout,
                "stderr": result.stderr or "",
            },
        )

    def __getattr__(self, operation):
        return lambda *args, **kwargs: self.invoke(operation, args, kwargs)


backend = Backend()
backend_name = "hyprland"
backend_display_name = "Hyprland"


def query(name, *, timeout=10, default=None, max_bytes=None):
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
        return backend.query(name, timeout=timeout, max_bytes=max_bytes)
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError):
        if default is None:
            raise
        return default


def config_errors(*, timeout=10, default=None):
    try:
        return backend.config_errors(timeout=timeout)
    except (OSError, subprocess.SubprocessError):
        if default is None:
            raise
        return default


def __getattr__(operation):
    return getattr(backend, operation)


# Lazy-import assertions inspect the actual installed client, not this adapter.
__file__ = str(ROOT / "nacre/shell-tools/nacre_native_compositor.py")
