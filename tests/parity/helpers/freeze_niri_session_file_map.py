#!/usr/bin/env python3
"""Freeze source/package installation map filtering before the native port."""

import importlib.util
import json
from pathlib import Path
import tempfile
from unittest.mock import patch

from freeze_niri_session_runtime import ROOT, verify_source

DESTINATION = (
    ROOT / "crates/nacre-helper/tests/fixtures/niri-session-file-map-reference.json"
)


def capture():
    verify_source()
    if DESTINATION.exists():
        raise ValueError("Refusing to overwrite file map reference")
    spec = importlib.util.spec_from_file_location(
        "map_reference", ROOT / "tools/configure.py"
    )
    configuration = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(configuration)
    sources = [
        "hypr/config.lua",
        "hypr/scripts/tool",
        "hypr/__pycache__/bad",
        "kitty/.qmlls.ini",
        "fish/bad.pyc",
        "uwsm/niri-shell-runtime.conf",
        "fastfetch/config.jsonc",
        "bin/orient",
        "bin/nacre-app",
        "bin/nacre-ai",
        "bin/nacre-brain",
        "nacre/desktop/nacre-ai.desktop",
        "nacre/desktop/nacre-brain.desktop",
        "nacre/desktop/nacre-settings.desktop",
        "nacre/shell-tools/nacre-power-key.service",
        "nacre/shell-tools/nacre-niri-widths.service",
        "ai/systemd/nacre-brain-sync.service",
    ]
    cases = []
    for components in (["configs", "shell"], ["configs", "shell", "ai", "brain"]):
        with tempfile.TemporaryDirectory(prefix="nacre-map-reference-") as folder:
            repo = Path(folder)
            for name in sources:
                path = repo / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("fixture\n")
            with patch.object(
                configuration,
                "selected_profile",
                return_value={"components": components},
            ):
                files = configuration.files(repo)
            cases.append(
                {
                    "components": components,
                    "files": {
                        str(target): str(source.relative_to(repo))
                        for target, source in files.items()
                    },
                }
            )
    with DESTINATION.open("x") as stream:
        json.dump({"inputs": sources, "cases": cases}, stream, indent=2)
        stream.write("\n")
    print(f"Froze {len(cases)} source installation maps")


if __name__ == "__main__":
    capture()
