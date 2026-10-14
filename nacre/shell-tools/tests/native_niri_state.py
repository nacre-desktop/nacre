#!/usr/bin/env python3
"""Keep every existing NiriStateTests assertion against the actual Rust reducer."""

import builtins
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
BINARY = Path(
    os.environ.get("NACRE_NIRI_STATE_PARITY")
    or ROOT / "target/debug/examples/niri-state-parity"
)


class NativeState:
    def __init__(self, outputs=None):
        self.outputs = outputs or {}
        self.workspaces = {}
        self.windows = {}
        self.ready = set()
        self.config_failed = False
        self.overview_open = False
        self.overview_hover_supported = False
        self.overview_hover_id = None

    def _call(self, operation, event=None):
        state = {**vars(self), "ready": sorted(self.ready)}
        result = subprocess.run(
            [
                str(BINARY),
                json.dumps({"state": state, "operation": operation, "event": event}),
            ],
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            check=True,
            timeout=10,
        )
        if result.stderr:
            raise RuntimeError(result.stderr)
        response = json.loads(result.stdout)
        for key, value in response["state"].items():
            setattr(self, key, set(value) if key == "ready" else value)
        result = response["result"]
        if "error" in result:
            raise getattr(builtins, result["type"], ValueError)(result["error"])
        return result["value"]

    def apply(self, event):
        return self._call("apply", event)

    def snapshot(self):
        snapshot = self._call("snapshot")
        active = snapshot.get("activeWindow")
        if active:
            snapshot["activeWindow"] = next(
                (
                    row
                    for row in snapshot["windows"]
                    if row["windowId"] == active["windowId"]
                ),
                active,
            )
        return snapshot
