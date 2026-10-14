"""Developer-only X11 fixture values; the width policy itself runs in Rust."""

import ctypes as c
import json
import math
import os
from pathlib import Path
import subprocess

MIN_SIZE = 1 << 4
MAX_SIZE = 1 << 5
ROOT = Path(__file__).resolve().parents[3]


class Pair(c.Structure):
    _fields_ = [("x", c.c_int), ("y", c.c_int)]


class SizeHints(c.Structure):
    _fields_ = (
        [("flags", c.c_long)]
        + [
            (name, c.c_int)
            for name in (
                "x",
                "y",
                "width",
                "height",
                "min_width",
                "min_height",
                "max_width",
                "max_height",
                "width_inc",
                "height_inc",
            )
        ]
        + [("min_aspect", Pair), ("max_aspect", Pair)]
        + [(name, c.c_int) for name in ("base_width", "base_height", "win_gravity")]
    )


def encode(hints):
    return (
        None
        if hints is None
        else {
            name: {"x": value.x, "y": value.y} if isinstance(value, Pair) else value
            for name, _ in SizeHints._fields_
            for value in [getattr(hints, name)]
        }
    )


def decode(value):
    hints = SizeHints()
    for name, item in value.items():
        setattr(hints, name, Pair(**item) if isinstance(item, dict) else item)
    return hints


class Policy:
    def __init__(self, x11):
        self.x11 = x11
        self.state = {}
        binary = os.environ.get("NACRE_WIDTHS_PARITY") or str(
            ROOT / "target/debug/examples/niri-widths-parity"
        )
        self.worker = subprocess.Popen(
            [binary],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )

    @property
    def windows(self):
        return {int(key): value for key, value in self.state.get("windows", {}).items()}

    def invoke(self, request, resize=None):
        self.worker.stdin.write(json.dumps(request) + "\n")
        self.worker.stdin.flush()
        while line := self.worker.stdout.readline():
            message = json.loads(line)
            if "result" in message:
                self.state = message["state"]
                result = message["result"]
                if "error" in result:
                    raise RuntimeError(result["error"])
                return result["value"]
            try:
                method = message["callback"]
                if method == "resize":
                    resize(message["id"])
                    value = None
                elif method == "set_hints":
                    self.x11.set_hints(message["window"], decode(message["hints"]))
                    value = None
                else:
                    value = getattr(self.x11, method)(
                        *([message["window"]] if "window" in message else [])
                    )
                    if method == "hints":
                        value = encode(value)
                    elif method == "identity" and value is not None:
                        value = [sorted(value[0]), value[1]]
                response = {"value": value}
            except (OSError, RuntimeError, ValueError) as error:
                response = {"error": str(error)}
            self.worker.stdin.write(json.dumps(response) + "\n")
            self.worker.stdin.flush()
        raise RuntimeError(self.worker.stderr.read())

    def event(self, event):
        return self.invoke({"operation": "event", "event": event})

    def reconcile(self):
        return self.invoke({"operation": "reconcile", "now": 100})

    def apply_openings(self, resize, now=None):
        now = 100 if now is None else now
        return self.invoke(
            {
                "operation": "apply_openings",
                "now": "infinity" if math.isinf(now) else now,
            },
            resize,
        )

    def close(self):
        return self.invoke({"operation": "close"})

    def stop(self):
        if self.worker.poll() is None:
            self.worker.stdin.write('{"operation":"stop"}\n')
            self.worker.stdin.flush()
        self.worker.communicate(timeout=5)
