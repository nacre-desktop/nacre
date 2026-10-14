#!/usr/bin/env python3
"""Freeze current session acceptance decisions and untouched records before port."""

import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from contextlib import redirect_stdout, redirect_stderr

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "uwsm/tools/niri-session-gate.py"
SOURCE_SHA = "85f5079ca1983a3f9b9e4ee5d210b8a5a66fd1b2d1dc1db2b8db43177f4cb582"
DESTINATION = (
    ROOT / "crates/nacre-helper/tests/fixtures/niri-session-gate-reference.json"
)


def verify_source():
    if (
        not SOURCE.is_file()
        or hashlib.sha256(SOURCE.read_bytes()).hexdigest() != SOURCE_SHA
    ):
        raise ValueError("Session gate reference source is absent or changed")


def requests():
    contexts = [
        {},
        {"XDG_CURRENT_DESKTOP": "Hyprland", "NIRI_SOCKET": "/stale"},
        {"XDG_CURRENT_DESKTOP": "niri", "HYPRLAND_INSTANCE_SIGNATURE": "stale"},
        {"HYPRLAND_INSTANCE_SIGNATURE": "fixture"},
        {"NIRI_SOCKET": "/fixture"},
        {"NIRI_SOCKET": "/fixture", "HYPRLAND_INSTANCE_SIGNATURE": "fixture"},
        {"XDG_CURRENT_DESKTOP": "niri:Hyprland"},
        {"XDG_CURRENT_DESKTOP": "niri:Hyprland", "NIRI_SOCKET": "/fixture"},
        {"NACRE_COMPOSITOR": "niri"},
    ]
    records = [
        None,
        "bad",
        "[]",
        '{"phase":"plain","backendReady":true}',
        '{"phase":"nacre","backendReady":true}',
        '{"phase":"nacre","backendReady":true,"observerReady":true}',
        '{"phase":"nacre","backendReady":true,"unlockVerified":true}',
        '{"phase":"nacre","backendReady":false,"observerReady":true,"unlockVerified":true}',
        '{"phase":"nacre","backendReady":1,"observerReady":true,"unlockVerified":true}',
        '{"phase":"nacre","backendReady":true,"observerReady":1,"unlockVerified":1}',
    ]
    for context_index, environment in enumerate(contexts):
        for record_index, record in enumerate(records):
            for action in ("shell", "observer", "idle"):
                yield {
                    "name": f"context-{context_index}-record-{record_index}-{action}",
                    "environment": environment,
                    "record": record,
                    "action": action,
                }


def child(request):
    verify_source()
    spec = importlib.util.spec_from_file_location("current_session_gate", SOURCE)
    gate = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(gate)
    home = Path(os.environ["HOME"])
    record = home / ".config/nacre/niri-on-pc/acceptance.json"
    if request["record"] is not None:
        record.parent.mkdir(parents=True)
        record.write_text(request["record"])
    before = record.read_bytes().hex() if record.exists() else None
    stdout, stderr = io.StringIO(), io.StringIO()
    with redirect_stdout(stdout), redirect_stderr(stderr):
        backend = gate.compositor(request["environment"])
        allowed = gate.allowed(request["action"], request["environment"], home)
    return {
        "backend": backend,
        "allowed": allowed,
        "exit": 0 if allowed else 1,
        "stdout": stdout.getvalue(),
        "stderr": stderr.getvalue(),
        "record_before": before,
        "record_after": record.read_bytes().hex() if record.exists() else None,
    }


def capture(destination=DESTINATION):
    verify_source()
    if destination.exists():
        raise ValueError("Refusing to overwrite frozen session gate reference")
    cases = []
    for request in requests():
        with tempfile.TemporaryDirectory(prefix="nacre-gate-reference-") as folder:
            env = dict(
                os.environ,
                HOME=folder,
                XDG_CONFIG_HOME=folder + "/.config",
                XDG_STATE_HOME=folder + "/.local/state",
                XDG_CACHE_HOME=folder + "/.cache",
                DBUS_SESSION_BUS_ADDRESS="unix:path=" + folder + "/absent-session",
                DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + folder + "/absent-system",
            )
            for key in (
                "DISPLAY",
                "WAYLAND_DISPLAY",
                "NIRI_SOCKET",
                "HYPRLAND_INSTANCE_SIGNATURE",
                "XDG_CURRENT_DESKTOP",
            ):
                env.pop(key, None)
            result = subprocess.run(
                [
                    sys.executable,
                    str(Path(__file__).resolve()),
                    "--child",
                    json.dumps(request),
                ],
                env=env,
                capture_output=True,
                text=True,
                check=True,
            )
            cases.append(
                {
                    "request": request,
                    "observed": json.loads(result.stdout),
                    "exit": result.returncode,
                    "stderr": result.stderr,
                }
            )
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("x") as stream:
        json.dump({"source_sha256": SOURCE_SHA, "cases": cases}, stream, indent=2)
        stream.write("\n")
    print(f"Froze {len(cases)} session gate cases")


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--child":
        print(json.dumps(child(json.loads(sys.argv[2]))))
    else:
        capture()
