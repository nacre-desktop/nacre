#!/usr/bin/env python3
"""Freeze original observer records and readiness policy before the Rust port."""

import datetime as dt
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile
from types import SimpleNamespace

from run import snapshot

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "nacre/shell-tools/session-watch.py"
DEST = ROOT / "crates/nacre-helper/tests/fixtures/session-watch-reference.json"
REAL_DATETIME = dt.datetime


class Fixed(REAL_DATETIME):
    @classmethod
    def now(cls, tz=None):
        return cls(2026, 10, 1, 12, 0, 0, tzinfo=tz)


def load():
    spec = importlib.util.spec_from_file_location("original_session_observer", SOURCE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main():
    if DEST.exists():
        raise ValueError("Refusing to replace frozen observer outputs")
    cases = []
    previous = dict(os.environ)
    for name, desktop, wallet in (
        ("unlocked", '{"launcherMode":"apps"}', "b false"),
        ("locked", '{"launcherMode":"apps"}', "b true"),
        ("unavailable", '{"launcherMode":"apps"}', ""),
        ("shell-not-ready", "invalid", "b false"),
    ):
        with tempfile.TemporaryDirectory(prefix="session-reference-") as folder:
            home = Path(folder)
            os.environ.update(
                HOME=folder,
                XDG_CONFIG_HOME=folder + "/.config",
                XDG_DATA_HOME=folder + "/.local/share",
                XDG_STATE_HOME=folder + "/.local/state",
                XDG_CACHE_HOME=folder + "/.cache",
                NACRE_COMPOSITOR="niri",
            )
            try:
                module = load()
                module.dt.datetime = Fixed
                monitors = [dict(name="DP-1", width=1920, height=1080, scale=1.25)]
                windows = [
                    dict(**{"class": value})
                    for value in ("nacre-brain", "chrome-127.0.0.1__-Default", "other")
                ]
                module.run = lambda args, *_, wallet=wallet, desktop=desktop, **__: (
                    wallet if args[0] == "busctl" else desktop
                )
                module.compositor = SimpleNamespace(
                    query=lambda kind, monitors=monitors, windows=windows, **_: (
                        monitors if kind == "monitors" else windows
                    ),
                    config_errors=lambda **_: "fixture error",
                )
                record = module.check("fixture")
                cases.append(
                    dict(
                        name=name,
                        inputs=dict(
                            desktop=desktop,
                            wallet=wallet,
                            monitors=monitors,
                            windows=windows,
                            configErrors="fixture error",
                        ),
                        result=record,
                        files=snapshot(home / ".local/state/nacre/session"),
                    )
                )
            finally:
                dt.datetime = REAL_DATETIME
                os.environ.clear()
                os.environ.update(previous)
    policies = []
    for name, states in (
        ("unavailable-is-ready", [dict(desktopReady=True, wallet="unavailable")]),
        (
            "locked-then-unlocked",
            [
                dict(desktopReady=True, wallet="locked"),
                dict(desktopReady=True, wallet="unlocked"),
            ],
        ),
        ("not-ready-timeout", [dict(desktopReady=False, wallet="unlocked")] * 3),
    ):
        module = load()
        clock = [0.0]
        events = []

        def check(event, events=events, states=states):
            events.append(event)
            return {**states[min(len(events) - 1, len(states) - 1)], "event": event}

        def sleep(seconds, clock=clock):
            clock[0] += seconds

        original_monotonic, original_sleep = module.time.monotonic, module.time.sleep
        try:
            module.check = check
            module.time.monotonic = lambda clock=clock: clock[0]
            module.time.sleep = sleep
            result = module.readiness(timeout=1)
            policies.append(
                dict(name=name, states=states, result=result, events=events)
            )
        finally:
            module.time.monotonic, module.time.sleep = (
                original_monotonic,
                original_sleep,
            )
    DEST.write_text(
        json.dumps(
            dict(
                source_sha256=hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
                clock="2026-10-01T12:00:00+00:00",
                records=cases,
                readiness=policies,
            ),
            indent=2,
        )
        + "\n"
    )
    print(
        "Frozen",
        len(cases),
        "observer file sets and",
        len(policies),
        "readiness policies",
    )


if __name__ == "__main__":
    main()
