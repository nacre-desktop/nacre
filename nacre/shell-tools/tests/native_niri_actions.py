#!/usr/bin/env python3
"""Run unchanged legacy action assertions with native logic and mock IPC only."""

import builtins
import copy
import json
import math
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
BINARY = Path(
    os.environ.get("NACRE_NIRI_ACTION_PARITY")
    or ROOT / "target/debug/examples/niri-action-parity"
)


def json_safe(value):
    # JSON cannot carry NaN/Infinity. Null reaches the same finite-value rejection.
    if isinstance(value, float) and not math.isfinite(value):
        return None
    if isinstance(value, (list, tuple)):
        return [json_safe(item) for item in value]
    if isinstance(value, dict):
        return {key: json_safe(item) for key, item in value.items()}
    return value


class NativeBackend:
    capabilities = {"pinWindow": False}

    def _state(self, timeout=10):
        raise RuntimeError("Legacy test must provide snapshot input")

    def _request(self, request, *, timeout=10):
        raise RuntimeError("Legacy test must provide mock IPC")

    def _window(self, value, timeout=10):
        snapshot = self._state(timeout)
        return (
            next((row for row in snapshot["windows"] if row["windowId"] == value), None)
            if value
            else snapshot["activeWindow"]
        )

    def _execute(self, method, args, kwargs):
        request = json_safe(
            {
                "method": method,
                "args": args,
                "kwargs": kwargs,
                "query_override": "query" in vars(self),
            }
        )
        with subprocess.Popen(
            [str(BINARY), json.dumps(request, allow_nan=False)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        ) as process:
            try:
                while line := process.stdout.readline():
                    message = json.loads(line)
                    if message.get("callback") == "query":
                        value = self.query(message["name"], timeout=message["timeout"])
                    elif message.get("callback") == "snapshot":
                        value = copy.deepcopy(self._state(message["timeout"]))
                    elif message.get("callback") == "window":
                        value = self._window(message["window"], message["timeout"])
                    elif message.get("callback") == "request":
                        if "_action" in vars(self) and "Action" in message["request"]:
                            action = message["request"]["Action"]
                            name = next(iter(action))
                            self._action(name, action[name], timeout=message["timeout"])
                            value = "Handled"
                        elif "focus_window" in vars(self) and message["request"].get(
                            "Action", {}
                        ).get("FocusWindow"):
                            identifier = message["request"]["Action"]["FocusWindow"][
                                "id"
                            ]
                            self.focus_window("niri-window:" + str(identifier))
                            value = "Handled"
                        else:
                            value = self._request(
                                message["request"], timeout=message["timeout"]
                            )
                    else:
                        result = message["result"]
                        if "error" in result:
                            raise getattr(builtins, result["type"], RuntimeError)(
                                result["error"]
                            )
                        return result["value"]
                    process.stdin.write(json.dumps(value, allow_nan=False) + "\n")
                    process.stdin.flush()
                raise RuntimeError(process.stderr.read())
            finally:
                process.stdin.close()
                process.wait(timeout=10)

    def __getattr__(self, method):
        return lambda *args, **kwargs: self._execute(method, args, kwargs)
