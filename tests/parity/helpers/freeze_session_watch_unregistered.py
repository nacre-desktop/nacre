#!/usr/bin/env python3
"""Freeze the original module's injected, unregistered evidence directory."""

import argparse
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
DEST = (
    ROOT
    / "crates/nacre-helper/tests/fixtures/session-watch-unregistered-reference.json"
)
REAL_DATETIME = dt.datetime


class Fixed(REAL_DATETIME):
    @classmethod
    def now(cls, tz=None):
        return cls(2026, 10, 1, 12, 0, 0, tzinfo=tz)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    args = parser.parse_args()
    if DEST.exists():
        raise ValueError("Refusing to replace frozen observer outputs")
    previous = dict(os.environ)
    with tempfile.TemporaryDirectory(prefix="session-reference-") as folder:
        try:
            os.environ.update(
                HOME=folder,
                XDG_CONFIG_HOME=folder + "/.config",
                XDG_STATE_HOME=folder + "/.local/state",
                XDG_DATA_HOME=folder + "/.local/share",
                XDG_CACHE_HOME=folder + "/.cache",
                NACRE_COMPOSITOR="niri",
                DBUS_SESSION_BUS_ADDRESS="unix:path=" + folder + "/absent-session",
                DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + folder + "/absent-system",
            )
            spec = importlib.util.spec_from_file_location(
                "original_session", args.source
            )
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            module.dt.datetime = Fixed
            module.STATE = Path(folder) / "override"
            module.run = lambda *_, **__: ""
            module.compositor = SimpleNamespace(
                query=lambda *_, **__: [], config_errors=lambda **_: ""
            )
            record = module.check("fixture")
            DEST.write_text(
                json.dumps(
                    dict(
                        source_sha256=hashlib.sha256(
                            args.source.read_bytes()
                        ).hexdigest(),
                        record=record,
                        files=snapshot(module.STATE),
                    ),
                    indent=2,
                )
                + "\n"
            )
            print("Frozen unregistered-directory record and every file/mode")
        finally:
            dt.datetime = REAL_DATETIME
            os.environ.clear()
            os.environ.update(previous)


if __name__ == "__main__":
    main()
