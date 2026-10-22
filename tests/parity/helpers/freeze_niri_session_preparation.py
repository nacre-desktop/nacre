#!/usr/bin/env python3
"""Freeze all prepared configuration writes before retiring the Python runtime.

Installation staging and keyboard discovery are explicit inputs here; their own
contracts are tested separately. Never contact the desktop or validate via IPC.
"""

import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import tempfile
from types import SimpleNamespace
from unittest.mock import patch

from freeze_niri_session_runtime import ROOT, SOURCE, SOURCE_SHA, verify_source

DESTINATION = (
    ROOT / "crates/nacre-helper/tests/fixtures/niri-session-preparation-reference.json"
)


def capture():
    verify_source()
    if DESTINATION.exists():
        raise ValueError("Refusing to overwrite preparation reference")
    spec = importlib.util.spec_from_file_location("reference_runtime", SOURCE)
    runtime = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runtime)
    original_module = runtime.module
    original_run = runtime.subprocess.run
    cases = []
    desktops = [
        {},
        {"naturalScroll": True, "displays": [{"name": "eDP-1", "scale": 1.5}]},
        {"gapsOut": 0, "borderSize": 0, "compositorDisplays": {"niri": []}},
        {"displays": [{"disabled": True, "name": "DP-1", "scale": False}]},
        {"displays": [{"name": "DP-1", "scale": True}]},
        {"naturalScroll": "yes"},
    ]
    with contextlib.ExitStack() as stack:
        isolated = stack.enter_context(
            tempfile.TemporaryDirectory(prefix="nacre-runtime-env-")
        )
        environment = dict(
            os.environ,
            HOME=isolated,
            NACRE_HELPER_BINARY=str(ROOT / "target/release/nacre-helper"),
            DBUS_SESSION_BUS_ADDRESS="unix:path=" + isolated + "/absent-session",
            DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + isolated + "/absent-system",
        )
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
        stack.enter_context(patch.dict(os.environ, environment, clear=True))
        for index, desktop in enumerate(desktops):
            for live in (False, True):
                with tempfile.TemporaryDirectory(
                    prefix="nacre-runtime-files-"
                ) as folder:
                    home = Path(folder)
                    destination = home / "stage"
                    destination.mkdir()
                    profile = json.loads((ROOT / "profiles/default.json").read_text())
                    profile["workspaceNames"] = ["One", "", 'Quoted "ø']
                    config = home / ".config/nacre"
                    config.mkdir(parents=True)
                    (config / "profile.json").write_text(json.dumps(profile))
                    (config / "desktop.json").write_text(json.dumps(desktop))
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
                    (state / "scheme.json").write_text(json.dumps(scheme))
                    if live:
                        (config / "niri").mkdir()
                        (config / "niri/palette.kdl").write_text("live palette\n")
                    calls = []
                    keyboard = {
                        "layout": "us,de",
                        "model": "pc105",
                        "variant": ",nodeadkeys",
                        "options": "caps:escape",
                    }

                    def stage(repo, target, selected, calls=calls):
                        calls.append({"operation": "stage", "profile": selected})
                        target.mkdir(parents=True)

                    def module(path, name, keyboard=keyboard):
                        if name == "session_prepare":
                            return SimpleNamespace(prepare=stage)
                        if name == "session_keyboard":
                            return SimpleNamespace(system_keyboard=lambda: keyboard)
                        return original_module(path, name)

                    def run(argv, calls=calls, **kwargs):
                        if argv[0] != "/usr/bin/niri":
                            return original_run(argv, **kwargs)
                        calls.append({"operation": "validate", "argv": argv})

                    stdout, stderr = io.StringIO(), io.StringIO()
                    error = None
                    with (
                        patch.object(runtime, "module", side_effect=module),
                        patch.object(runtime.subprocess, "run", side_effect=run),
                        patch.object(
                            runtime.subprocess,
                            "check_output",
                            return_value="fixture-revision\n",
                        ),
                        contextlib.redirect_stdout(stdout),
                        contextlib.redirect_stderr(stderr),
                    ):
                        try:
                            runtime.prepare(ROOT, destination, home, stock_niri=live)
                        except (ValueError, TypeError, KeyError, OSError) as failure:
                            error = {
                                "type": type(failure).__name__,
                                "message": str(failure),
                            }
                    files = {
                        str(path.relative_to(destination)): {
                            "text": path.read_text(),
                            "mode": path.stat().st_mode & 0o777,
                        }
                        for path in sorted(destination.rglob("*"))
                        if path.is_file()
                    }
                    directories = {
                        str(path.relative_to(destination)): path.stat().st_mode & 0o777
                        for path in sorted(destination.rglob("*"))
                        if path.is_dir()
                    }
                    record = {
                        "name": f"prepare-{index}-{live}",
                        "profile": profile,
                        "desktop": desktop,
                        "scheme": scheme,
                        "keyboard": keyboard,
                        "live": live,
                        "stock": live,
                        "observed": {
                            "files": files,
                            "directories": directories,
                            "calls": calls,
                            "stdout": stdout.getvalue(),
                            "stderr": stderr.getvalue(),
                            "error": error,
                        },
                    }
                    normalized = json.dumps(record).replace(str(home), "$HOME")
                    normalized = normalized.replace(str(ROOT) + os.sep, "$REPO/")
                    cases.append(json.loads(normalized))
    with DESTINATION.open("x") as stream:
        json.dump({"source_sha256": SOURCE_SHA, "cases": cases}, stream, indent=2)
        stream.write("\n")
    print(f"Froze {len(cases)} preparation file-write cases")


if __name__ == "__main__":
    capture()
