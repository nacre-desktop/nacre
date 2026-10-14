#!/usr/bin/env python3
"""Freeze current menu cache policy with private state and a fixed clock."""

import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
from unittest.mock import patch

SOURCE_SHA = "d74ca0b2a6d6d41a173b399fa5fbf876472d9ef8b18572d604ea8b20108eb958"


def capture(source, target):
    if target.exists():
        raise ValueError("Refusing to overwrite the menu cache reference")
    if hashlib.sha256(source.read_bytes()).hexdigest() != SOURCE_SHA:
        raise ValueError("Current menu reference source changed")
    cases = []
    with tempfile.TemporaryDirectory(prefix="menu-cache-reference-") as directory:
        os.environ["HOME"] = directory
        os.environ["NACRE_COMPOSITOR"] = "hyprland"
        sys.path.insert(0, str(source.parent))
        spec = importlib.util.spec_from_file_location("menu_cache_reference", source)
        menu = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(menu)
        for name, stamps, times, force in [
            ("hit", [1, 1], [1000, 1001], [True, False]),
            ("package-change", [1, 2], [1000, 1001], [True, False]),
            ("expired", [1, 1], [1000, 1030], [True, False]),
            ("clock-backwards", [1, 1], [1000, 999], [True, False]),
            ("forced", [1, 1], [1000, 1001], [True, True]),
        ]:
            home = Path(directory) / name
            home.mkdir()
            calls = []
            available = False

            def installed(calls=calls):
                calls.append("installed")
                return []

            def fonts(calls=calls):
                calls.append("fonts")
                return ["Fixture Sans"]

            results = []
            with (
                patch.object(menu, "HOME", home),
                patch.object(menu, "CACHE", home / ".cache/nacre/menu"),
                patch.object(menu, "installed", side_effect=installed),
                patch.object(menu, "fonts", side_effect=fonts),
                patch.object(menu, "active", return_value=False),
                patch.object(menu, "reminders", return_value=[]),
                patch.object(menu, "webapps", return_value=[]),
            ):
                for index in range(2):
                    with (
                        patch.object(menu, "package_stamp", return_value=stamps[index]),
                        patch.object(menu.time, "time", return_value=times[index]),
                        patch.object(
                            menu.shutil,
                            "which",
                            return_value="/fixture/tool" if available else None,
                        ),
                    ):
                        results.append(menu.snapshot(force[index]))
                    available = True
            cache = home / ".cache/nacre/menu/state.json"
            cases.append(
                dict(
                    name=name,
                    stamps=stamps,
                    times=times,
                    force=force,
                    results=results,
                    calls=calls,
                    cache=cache.read_text(),
                    mode=oct(cache.stat().st_mode & 0o777),
                )
            )
    with target.open("x") as output:
        output.write(
            json.dumps(dict(source_sha256=SOURCE_SHA, cases=cases), indent=2) + "\n"
        )
    return len(cases)


if __name__ == "__main__":
    print("Frozen menu cache cases:", capture(Path(sys.argv[1]), Path(sys.argv[2])))
