#!/usr/bin/env python3
"""Freeze the current profile validator needed by prepared-session staging."""

import importlib.util
import json
import os
import tempfile
from unittest.mock import patch

from freeze_niri_session_runtime import ROOT, verify_source

DESTINATION = (
    ROOT / "crates/nacre-helper/tests/fixtures/niri-session-profile-reference.json"
)


def capture():
    verify_source()
    if DESTINATION.exists():
        raise ValueError("Refusing to overwrite profile reference")
    spec = importlib.util.spec_from_file_location(
        "current_profile", ROOT / "nacre/shell-tools/profile-policy.py"
    )
    policy = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(policy)
    requests = [
        None,
        [],
        {},
        {"unknown": True},
        {"version": True},
        {"version": 1.0},
        {"version": 2},
        {"id": "Upper"},
        {"workspaceCount": True},
        {"workspaceCount": 0},
        {"workspaceCount": 11},
        {"workspaceCount": 1.0},
        {"components": ["shell", "shell"]},
        {"components": ["unknown"]},
        {"components": "shell"},
        {"workspaceNames": [5]},
        {"workspaceNames": ["a\0b"]},
        {"workspaceNames": ["a" * 101]},
        {"brainWorkspace": True},
        {"brainWorkspace": 20},
        {"aiFullAccess": 1},
        {"windowTrashBindings": "true"},
        {"startup": {}},
        {"startup": [{}]},
        {
            "startup": [
                {
                    "label": "Test",
                    "workspace": 1,
                    "argv": ["kitty"],
                    "classes": ["kitty"],
                }
            ]
        },
        {"routes": [{"name": "Rule", "match": "app", "workspace": 2}]},
        {"routes": [{"name": "Rule\n", "match": "app", "workspace": 2}]},
        {
            "startup": [
                {"label": "Test", "workspace": 1, "argv": "kitty", "classes": []}
            ]
        },
        {
            "startup": [
                {"label": "Test", "workspace": 1, "argv": ["kitty"], "classes": [3]}
            ]
        },
    ]
    cases = []
    with (
        tempfile.TemporaryDirectory(prefix="nacre-profile-reference-") as home,
        patch.dict(os.environ, {"HOME": home, "LANG": "C", "NACRE_LANG": "en"}),
    ):
        for request in requests:
            try:
                observed = {"value": policy.validate(request), "error": None}
            except (ValueError, TypeError, KeyError) as error:
                observed = {
                    "value": None,
                    "error": {"type": type(error).__name__, "message": str(error)},
                }
            cases.append({"request": request, "observed": observed})
    with DESTINATION.open("x") as stream:
        json.dump({"cases": cases}, stream, indent=2)
        stream.write("\n")
    print(f"Froze {len(cases)} profile validation cases")


if __name__ == "__main__":
    capture()
