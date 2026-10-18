#!/usr/bin/env python3
"""Freeze the additive Niri preferences without replacing any older reference."""

import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "nacre/shell-tools/desktop-settings.py"
DEST = ROOT / "crates/nacre-helper/tests/fixtures/desktop-settings-niri-reference.json"
KEYS = ("monitorWorkspaceActions", "niriWorkspaceMode", "workspacePrimaryOutput")


def main():
    if DEST.exists():
        raise ValueError("Refusing to replace frozen Niri settings outputs")
    previous = dict(os.environ)
    with tempfile.TemporaryDirectory(prefix="nacre-settings-niri-reference-") as folder:
        try:
            os.environ.update(
                HOME=folder,
                XDG_CONFIG_HOME=folder + "/.config",
                XDG_STATE_HOME=folder + "/.local/state",
                XDG_CACHE_HOME=folder + "/.cache",
                XDG_DATA_HOME=folder + "/.local/share",
                XDG_RUNTIME_DIR=folder + "/runtime",
                DBUS_SESSION_BUS_ADDRESS="unix:path=" + folder + "/absent-session",
                DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + folder + "/absent-system",
                NACRE_COMPOSITOR="niri",
                NACRE_DEVELOPER_MODE="0",
            )
            sys.path.insert(0, str(SOURCE.parent))
            spec = importlib.util.spec_from_file_location("reference_settings", SOURCE)
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            cases = []
            for key, values in (
                ("monitorWorkspaceActions", [False, True, 0, 1, "false", None]),
                (
                    "niriWorkspaceMode",
                    ["primary-dynamic", "fixed-all", "dynamic", "", True, None],
                ),
                (
                    "workspacePrimaryOutput",
                    [
                        "",
                        "DP-1",
                        "A" * 80,
                        "A" * 81,
                        "line\nbreak",
                        "x\x7f",
                        "Δ" * 80,
                        True,
                        None,
                    ],
                ),
            ):
                for value in values:
                    case = dict(key=key, value=value)
                    try:
                        case["result"] = module.validate(key, value)
                    except (ValueError, TypeError, KeyError) as error:
                        case.update(error=str(error), type=type(error).__name__)
                    cases.append(case)
            module.compositor._close()
            files = {
                str(path.relative_to(folder)): path.read_text()
                for path in sorted(Path(folder).rglob("*"))
                if path.is_file()
            }
            result = dict(
                source_sha256=hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
                defaults={key: module.DEFAULTS[key] for key in KEYS},
                excludedFromPresets=[
                    key for key in KEYS if key not in module.PRESET_KEYS
                ],
                validation=cases,
                files=files,
                intendedDifferences=[
                    "Absolute custom XDG roots follow NacrePaths.qml."
                ],
            )
            DEST.parent.mkdir(parents=True, exist_ok=True)
            DEST.write_text(json.dumps(result, indent=2) + "\n")
            print(
                f"Frozen {len(cases)} additive validation cases; {len(files)} files written"
            )
        finally:
            os.environ.clear()
            os.environ.update(previous)


if __name__ == "__main__":
    main()
