#!/usr/bin/env python3
"""Freeze KDE palette files from the current Python implementation once."""

import configparser
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "nacre/shell-tools/kde-palette.py"
SOURCE_SHA = "820e12f97eceaa3932f62b2ae80b5dc295d79301821b37e58485706d7cbb4dfa"
DESTINATION = ROOT / "crates/nacre-helper/tests/fixtures/kde-palette-reference.json"


def capture():
    if DESTINATION.exists():
        raise ValueError("Refusing to overwrite KDE palette reference")
    source_sha = hashlib.sha256(SOURCE.read_bytes()).hexdigest()
    if source_sha != SOURCE_SHA:
        raise ValueError("KDE palette source changed")
    spec = importlib.util.spec_from_file_location("reference_kde", SOURCE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    colors = json.loads((SOURCE.parent / "reference-style.json").read_text())["colours"]
    seeds = [
        "",
        "[General]\nTerminalApplication=kitty\nColorScheme=Custom\n[UserSection]\nSetting=preserve\n",
        "[Icons]\nTheme=Custom\n[Colors:View]\nForegroundNormal=old\nUnrelated=keep\n",
        "[DEFAULT]\nInherited=kept\n[Colors:View]\nBackgroundNormal=#000000\n",
        "# comment\n[General]\nKey=first\nKey: second\n[General]\nOther=value\n",
        "[Custom]\nMultiline=first\n second\n third\nPercent=100%\n",
        "invalid header\nkey=value\n",
    ]
    cases = []
    for seed in seeds:
        for icon in ("", "ExampleIcons"):
            with tempfile.TemporaryDirectory(prefix="nacre-kde-reference-") as folder:
                home = Path(folder)
                if seed:
                    (home / ".config").mkdir()
                    (home / ".config/kdeglobals").write_text(seed)
                error = None
                try:
                    module.publish(home, colors, icon)
                except (OSError, ValueError, configparser.Error) as failure:
                    error = type(failure).__name__
                files = {}
                for file in sorted(home.rglob("*")):
                    if file.is_file():
                        files[str(file.relative_to(home))] = {
                            "text": file.read_text(),
                            "mode": file.stat().st_mode & 0o777,
                        }
                cases.append(
                    {
                        "seed": seed,
                        "colors": colors,
                        "icon": icon,
                        "error": error,
                        "files": files,
                    }
                )
    DESTINATION.write_text(
        json.dumps({"source_sha256": source_sha, "cases": cases}, indent=2) + "\n"
    )
    print(f"Frozen {len(cases)} KDE file cases")


if __name__ == "__main__":
    capture()
