#!/usr/bin/env python3
"""Freeze complete staging results using a small, declarative source tree."""

import contextlib
import io
import json
import os
from pathlib import Path
import tempfile
from types import SimpleNamespace
from unittest.mock import patch

from freeze_niri_session_runtime import ROOT, verify_source

DESTINATION = (
    ROOT / "crates/nacre-helper/tests/fixtures/niri-session-stage-reference.json"
)


def capture():
    verify_source()
    if DESTINATION.exists():
        raise ValueError("Refusing to overwrite stage reference")
    import importlib.util

    spec = importlib.util.spec_from_file_location(
        "stage_reference", ROOT / "tools/prepare.py"
    )
    stage = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(stage)
    original_module = stage.module
    inputs = {
        "VERSION": ["0.0.fixture\n", 0o644],
        "hypr/config.lua": ["config fixture\n", 0o640],
        "bin/nacre-app": ["#!/bin/sh\nexit 0\n", 0o755],
        "bin/nacre-ai": ["ai fixture\n", 0o700],
        "nacre/shell/shell.qml": ["shell fixture\n", 0o644],
        "nacre/shell/__pycache__/ignored.pyc": ["ignored", 0o644],
        "nacre/shell-tools/helper.json": ["{}\n", 0o600],
        "nacre/shell-tools/tests/ignored.py": ["ignored", 0o644],
        "nacre/shell-tools/nacre-helper": ["native binary fixture\n", 0o755],
        "nacre/shell-tools/nacre-shell.service": ["shell unit\n", 0o644],
        "nacre/shell-tools/nacre-niri-widths.service": ["widths unit\n", 0o644],
        "nacre/shell-tools/nacre-sidebar-ai.service": ["ai unit\n", 0o644],
        "brain/control.py": ["brain fixture\n", 0o644],
        "brain/brain.service": ["brain unit\n", 0o644],
        "brain/tests/ignored.py": ["ignored", 0o644],
    }
    cases = []
    for components in (
        ["configs", "shell"],
        ["configs", "shell", "ai"],
        ["configs", "shell", "brain"],
        ["configs", "shell", "ai", "brain"],
    ):
        with tempfile.TemporaryDirectory(prefix="nacre-stage-reference-") as folder:
            root = Path(folder)
            repo, destination = root / "repo", root / "destination"
            for name, (text, mode) in inputs.items():
                path = repo / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text)
                path.chmod(mode)
            # Production validation and Lua rendering stay real; only file-map
            # discovery is an input so a future map change cannot rewrite goldens.
            selected = {"components": components}
            sources = {
                Path(".config/hypr/config.lua"): repo / "hypr/config.lua",
                Path(".local/bin/nacre-app"): repo / "bin/nacre-app",
            }
            if "ai" in components:
                sources[Path(".local/bin/nacre-ai")] = repo / "bin/nacre-ai"
            configuration = SimpleNamespace(
                selected_profile=None, files=lambda repo, sources=sources: sources
            )

            def module(path, name, configuration=configuration):
                if name == "prepare_configuration":
                    return configuration
                return original_module(
                    ROOT / "nacre/shell-tools/profile-policy.py", name
                )

            stdout = io.StringIO()
            environment = dict(
                os.environ,
                HOME=str(root / "home"),
                NACRE_HELPER_BINARY=str(ROOT / "target/release/nacre-helper"),
            )
            for key in (
                "DISPLAY",
                "WAYLAND_DISPLAY",
                "NIRI_SOCKET",
                "HYPRLAND_INSTANCE_SIGNATURE",
                "XDG_CURRENT_DESKTOP",
            ):
                environment.pop(key, None)
            with (
                patch.dict(os.environ, environment, clear=True),
                patch.object(stage, "module", side_effect=module),
                contextlib.redirect_stdout(stdout),
            ):
                stage.prepare(repo, destination, selected)
            files = {
                str(path.relative_to(destination)): {
                    "text": path.read_text(),
                    "mode": path.stat().st_mode & 0o777,
                }
                for path in sorted(destination.rglob("*"))
                if path.is_file()
            }
            cases.append(
                {
                    "profile": selected,
                    "map": {
                        str(target): str(source.relative_to(repo))
                        for target, source in sources.items()
                    },
                    "files": files,
                    "stdout": stdout.getvalue().replace(
                        str(destination), "$DESTINATION"
                    ),
                }
            )
    with DESTINATION.open("x") as stream:
        json.dump({"inputs": inputs, "cases": cases}, stream, indent=2)
        stream.write("\n")
    print(f"Froze {len(cases)} complete staging cases")


if __name__ == "__main__":
    capture()
