#!/usr/bin/env python3
"""Freeze pre-port capture contracts with private files and injected tools only."""

import importlib.util
import hashlib
import json
import os
from datetime import datetime
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
from unittest.mock import patch


def freeze(source):
    if (
        hashlib.sha256(source.read_bytes()).hexdigest()
        != "d74ca0b2a6d6d41a173b399fa5fbf876472d9ef8b18572d604ea8b20108eb958"
    ):
        raise ValueError("Capture reference source differs from frozen CURRENT Python")
    cases = []
    with tempfile.TemporaryDirectory(prefix="nacre-capture-reference-") as folder:
        os.environ["HOME"] = folder
        os.environ["XDG_CONFIG_HOME"] = folder + "/.config"
        os.environ["NACRE_COMPOSITOR"] = "hyprland"
        sys.path.insert(0, str(source.parent))
        spec = importlib.util.spec_from_file_location("capture_reference", source)
        menu = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(menu)
        import screenshot

        class FixedDate:
            @staticmethod
            def now():
                return datetime(2024, 1, 2, 3, 4, 5, 678901)

        inputs = [
            ("capture-window", False, {}, 0, ""),
            ("capture-window", False, {"at": [0, 0], "size": [0, 10]}, 0, ""),
            ("capture-window", False, {"at": [True, 0], "size": [10, 10]}, 0, ""),
            ("capture-window", False, {"at": [0, 0], "size": [32769, 10]}, 0, ""),
            ("capture-window", False, {"at": [-20, 30], "size": [200, 100]}, 0, ""),
            ("capture-window", True, {}, 0, ""),
            ("capture-window", True, {"windowId": "niri:42"}, 0, ""),
            ("capture-area", False, {}, 1, ""),
            ("capture-area", False, {}, 0, ""),
            ("capture-area", False, {}, 0, " 10,20 30x40\n"),
            ("capture-screen", False, {}, 0, ""),
        ]
        for action, native, current, code, selection in inputs:
            home = Path(folder) / str(len(cases))
            home.mkdir()
            calls = []

            def normalize(value, home=home):
                return str(value).replace(str(home), "{home}")

            def run(argv, calls=calls, selection=selection, code=code, **kwargs):
                payload = kwargs.get("input", b"")
                calls.append(
                    dict(
                        argv=[normalize(x) for x in argv],
                        input=payload.hex(),
                        check=kwargs.get("check", False),
                        capture_output=kwargs.get("capture_output", False),
                        text=kwargs.get("text", False),
                    )
                )
                if argv[0] == "grim" and argv[-1] != "-":
                    Path(argv[-1]).write_bytes(b"fixture png")
                stdout = (
                    selection
                    if argv[0] == "slurp"
                    else b"fixture png"
                    if argv[0] in ("grim", "wl-paste")
                    else b""
                )
                return SimpleNamespace(
                    returncode=code if argv[0] == "slurp" else 0, stdout=stdout
                )

            def query(name, calls=calls, current=current, **kwargs):
                calls.append(dict(query=name, timeout=kwargs["timeout"]))
                return current

            def screenshot_window(path, window, calls=calls, **kwargs):
                calls.append(
                    dict(path=normalize(path), window=window, timeout=kwargs["timeout"])
                )
                path.write_bytes(b"fixture png")

            with (
                patch.object(menu, "HOME", home),
                patch.object(menu, "datetime", FixedDate),
                patch.object(screenshot, "datetime", FixedDate),
                patch.object(screenshot.Path, "home", return_value=home),
                patch.object(menu.compositor, "query", side_effect=query),
                patch.object(
                    menu.compositor, "capabilities", {"nativeWindowScreenshot": native}
                ),
                patch.object(
                    menu.compositor, "screenshot_window", side_effect=screenshot_window
                ),
                patch.object(menu.subprocess, "run", side_effect=run),
            ):
                # screenshot.capture's default runner was bound before patching.
                capture = screenshot.capture
                with patch.object(
                    screenshot,
                    "capture",
                    side_effect=lambda mode, capture=capture, home=home, run=run: (
                        capture(mode, home, run)
                    ),
                ):
                    try:
                        outcome = dict(value=menu.tool(action))
                    except ValueError as error:
                        outcome = dict(error="ValueError", message=str(error))
            files = {
                str(path.relative_to(home)): dict(
                    bytes=path.read_bytes().hex(), mode=oct(path.stat().st_mode & 0o777)
                )
                for path in home.rglob("*")
                if path.is_file()
            }
            cases.append(
                dict(
                    action=action,
                    native=native,
                    current=current,
                    code=code,
                    selection=selection,
                    outcome=outcome,
                    calls=calls,
                    files=files,
                )
            )
    return dict(
        reference="CURRENT Python menu capture; private HOME, fake tools and compositor; fixed date 2024-01-02T03:04:05.678901",
        cases=cases,
    )


if __name__ == "__main__":
    source, target = map(Path, sys.argv[1:])
    if target.exists():
        raise ValueError("Refusing to overwrite an existing capture reference")
    target.write_text(json.dumps(freeze(source), indent=2) + "\n")
    print("Frozen capture cases: 11")
