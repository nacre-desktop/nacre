#!/usr/bin/env python3
"""Freeze every preview file/symlink write, with staging/keyboard inputs explicit."""

import contextlib
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import tempfile
from types import SimpleNamespace
from unittest.mock import patch

from freeze_niri_preview import ROOT, SOURCE, SOURCE_SHA

DESTINATION = (
    ROOT / "crates/nacre-helper/tests/fixtures/niri-preview-preparation-reference.json"
)


def capture():
    if hashlib.sha256(SOURCE.read_bytes()).hexdigest() != SOURCE_SHA:
        raise ValueError("Preview source changed")
    if DESTINATION.exists():
        raise ValueError("Refusing to overwrite preview preparation reference")
    spec = importlib.util.spec_from_file_location("preview_files_reference", SOURCE)
    preview = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(preview)
    original_module = preview.module
    cases = []
    for variant in ("default", "named", "full-access", "invalid-palette", "nonempty"):
        with tempfile.TemporaryDirectory(prefix="nacre-preview-reference-") as folder:
            home = Path(folder)
            destination = home / "preview"
            profile = json.loads((ROOT / "profiles/default.json").read_text())
            if variant == "named":
                profile.update(
                    workspaceCount=3, workspaceNames=["One", 'Quoted "ø', ""]
                )
            if variant == "full-access":
                profile["aiFullAccess"] = True
            config = home / ".config/nacre"
            config.mkdir(parents=True)
            files = {
                "profile.json": json.dumps(profile),
                "desktop.json": "{}\n",
                "profile.lua": "custom fixture\n",
                "shortcuts.lua": "custom shortcuts\n",
                "auth.json": "excluded credentials fixture\n",
            }
            for name, text in files.items():
                (config / name).write_text(text)
            state = home / ".local/state/nacre"
            state.mkdir(parents=True)
            scheme = {
                "colours": {
                    "primary": "#abcdef",
                    "secondary": "12345678",
                    "outlineVariant": "#ABCDEF",
                    "error": "123abc",
                }
            }
            if variant == "invalid-palette":
                scheme["colours"]["error"] = "invalid"
            (state / "scheme.json").write_text(json.dumps(scheme))
            cache = home / ".cache/nacre"
            cache.mkdir(parents=True)
            (cache / "poster.png").write_text("image fixture\n")
            (cache / "weather.json").write_text("weather fixture\n")
            (cache / "unrelated.json").write_text("unrelated fixture\n")
            (state / "presentation.json").write_text(
                json.dumps({"poster": str(cache / "poster.png")})
            )
            share = home / ".local/share/nacre"
            (share / "shell/bin").mkdir(parents=True)
            (share / "shell/bin/qs").write_text("qs fixture\n")
            for name in ("palette-runtime", "branding", "ai", "control"):
                (share / name).mkdir()
            if variant == "nonempty":
                destination.mkdir()
                (destination / "existing").write_text("keep existing\n")
            keyboard = {
                "layout": "us,de",
                "model": "pc105",
                "variant": ",nodeadkeys",
                "options": "caps:escape",
            }
            calls = []

            def stage(repo, target, selected, calls=calls):
                calls.append({"operation": "stage", "profile": selected})
                (target / ".config/nacre").mkdir(parents=True)
                (target / ".local/share/nacre/shell/tools").mkdir(parents=True)
                print("fixture staging\n", end="")

            def module(path, name, keyboard=keyboard):
                if name == "niri_preview_prepare":
                    return SimpleNamespace(prepare=stage)
                if name == "niri_preview_keyboard":
                    return SimpleNamespace(system_keyboard=lambda: keyboard)
                return original_module(path, name)

            environment = {
                **os.environ,
                "HOME": str(home),
                "NACRE_HELPER_BINARY": str(ROOT / "target/release/nacre-helper"),
                "DBUS_SESSION_BUS_ADDRESS": "unix:path=" + str(home / "absent-session"),
                "DBUS_SYSTEM_BUS_ADDRESS": "unix:path=" + str(home / "absent-system"),
            }
            for name in (
                "DISPLAY",
                "WAYLAND_DISPLAY",
                "NIRI_SOCKET",
                "HYPRLAND_INSTANCE_SIGNATURE",
                "XDG_CURRENT_DESKTOP",
                "XDG_CONFIG_HOME",
                "XDG_STATE_HOME",
                "XDG_CACHE_HOME",
            ):
                environment.pop(name, None)
            stdout, stderr = io.StringIO(), io.StringIO()
            error = None
            with (
                patch.dict(os.environ, environment, clear=True),
                patch.object(preview, "module", side_effect=module),
                patch.object(
                    preview.subprocess,
                    "check_output",
                    return_value="fixture-revision\n",
                ),
                contextlib.redirect_stdout(stdout),
                contextlib.redirect_stderr(stderr),
            ):
                try:
                    preview.prepare(ROOT, destination, home)
                except (ValueError, TypeError, KeyError, OSError) as failure:
                    error = {"type": type(failure).__name__, "message": str(failure)}
            written = {}
            directories = {}
            for path in sorted(destination.rglob("*")):
                name = str(path.relative_to(destination))
                if path.is_symlink():
                    written[name] = {"symlink": str(path.readlink())}
                elif path.is_file():
                    written[name] = {
                        "text": path.read_text(),
                        "mode": path.stat().st_mode & 0o777,
                    }
                elif path.is_dir():
                    directories[name] = path.stat().st_mode & 0o777
            case = {
                "variant": variant,
                "profile": profile,
                "scheme": scheme,
                "keyboard": keyboard,
                "observed": {
                    "files": written,
                    "directories": directories,
                    "calls": calls,
                    "stdout": stdout.getvalue(),
                    "stderr": stderr.getvalue(),
                    "error": error,
                },
            }
            cases.append(
                json.loads(
                    json.dumps(case)
                    .replace(str(home), "$HOME")
                    .replace(str(ROOT), "$REPO")
                )
            )
    with DESTINATION.open("x") as stream:
        json.dump({"source_sha256": SOURCE_SHA, "cases": cases}, stream, indent=2)
        stream.write("\n")
    print(f"Froze {len(cases)} preview preparation file-write cases")


if __name__ == "__main__":
    capture()
