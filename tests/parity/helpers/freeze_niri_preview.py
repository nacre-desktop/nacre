#!/usr/bin/env python3
"""Freeze the current preview namespace, safe copies and generated aliases."""

import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "uwsm/tools/niri-preview.py"
SOURCE_SHA = "e535840f4a96a2878c977a64711f881270373d7ef729bbb6299065c22d79ee69"
DESTINATION = ROOT / "crates/nacre-helper/tests/fixtures/niri-preview-reference.json"


def capture():
    if (
        not SOURCE.is_file()
        or hashlib.sha256(SOURCE.read_bytes()).hexdigest() != SOURCE_SHA
    ):
        raise ValueError("Preview source is absent or changed")
    if DESTINATION.exists():
        raise ValueError("Refusing to overwrite preview reference")
    spec = importlib.util.spec_from_file_location("preview_reference", SOURCE)
    preview = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(preview)
    cases = [
        {
            "operation": "allowlists",
            "observed": {"config": preview.SAFE_CONFIG, "state": preview.SAFE_STATE},
        }
    ]
    for display in ("", "wayland-1", "/private/parent/wayland-1"):
        for managed in (False, True):
            for which in (None, "/fixture/quickshell"):
                with tempfile.TemporaryDirectory(
                    prefix="nacre-preview-command-"
                ) as folder:
                    home = Path(folder)
                    environment = {
                        "HOME": str(home),
                        "WAYLAND_DISPLAY": display,
                        "XDG_RUNTIME_DIR": "/private/parent",
                        "HYPRLAND_INSTANCE_SIGNATURE": "daily",
                        "NIRI_SOCKET": "daily",
                        "DBUS_SESSION_BUS_ADDRESS": "daily",
                        "DBUS_SYSTEM_BUS_ADDRESS": "unix:path=/outside/system",
                        "DISPLAY": ":0",
                        "NOTIFY_SOCKET": "daily",
                        "GDK_BACKEND": "wayland",
                        "PATH": "/private/bin",
                    }
                    if managed:
                        share = home / ".local/share/nacre"
                        share.mkdir(parents=True)
                        for name in ("palette-runtime", "branding", "ai", "control"):
                            release = home / ("release-" + name)
                            release.mkdir()
                            (share / name).symlink_to(release, target_is_directory=True)
                    error = None
                    with patch.object(preview.shutil, "which", return_value=which):
                        try:
                            argv, child = preview.command(
                                home / "preview", home, environment, home / "runtime"
                            )
                            observed = {"argv": argv, "environment": child}
                        except ValueError as failure:
                            observed = None
                            error = str(failure)
                    case = {
                        "operation": "command",
                        "display": display,
                        "managed": managed,
                        "which": which,
                        "environment": environment,
                        "observed": observed,
                        "error": error,
                    }
                    cases.append(
                        json.loads(json.dumps(case).replace(str(home), "$HOME"))
                    )
    for names in (
        {},
        {1: "One", 2: "", 10: "Ten"},
        {1: 'Quoted "ø', 2: "personal workspace"},
    ):
        cases.append(
            {
                "operation": "preview_bindings",
                "names": names,
                "home": '/fixture/quoted"home',
                "observed": preview.preview_bindings(
                    Path('/fixture/quoted"home'), names
                ),
            }
        )
    for variant in ("missing", "file", "directory", "wallpaper-link", "cache-scope"):
        with tempfile.TemporaryDirectory(prefix="nacre-preview-copies-") as folder:
            home = Path(folder)
            target = home / "snapshot"
            if variant == "cache-scope":
                cache = home / ".cache/nacre"
                cache.mkdir(parents=True)
                (cache / "poster.png").write_text("image fixture\n")
                (cache / "unrelated.json").write_text("unrelated\n")
                (home / "private.txt").write_text("outside\n")
                (cache / "escape.png").symlink_to(home / "private.txt")
                state = home / "state"
                state.mkdir()
                (state / "presentation.json").write_text(
                    json.dumps(
                        {
                            "poster": str(cache / "poster.png"),
                            "escape": str(cache / "escape.png"),
                            "nested": [str(cache / "poster.png")],
                        }
                    )
                )
                preview.copy_cached_presentation(home, state, target)
            else:
                source = home / "source"
                if variant == "file":
                    source.write_text('{"aiFullAccess":true}\n')
                    source.chmod(0o640)
                elif variant in ("directory", "wallpaper-link"):
                    source.mkdir()
                    (source / "data.json").write_text("fixture\n")
                    (source / "data.json").chmod(0o640)
                    if variant == "wallpaper-link":
                        asset = home / "poster.png"
                        asset.write_text("image fixture\n")
                        (source / "current").symlink_to(asset)
                preview.private_copy(source, target)
            if target.is_file():
                files = {
                    ".": {
                        "text": target.read_text(),
                        "mode": target.stat().st_mode & 0o777,
                        "symlink": target.is_symlink(),
                    }
                }
            else:
                files = {
                    str(path.relative_to(target)): {
                        "text": path.read_text(),
                        "mode": path.stat().st_mode & 0o777,
                        "symlink": path.is_symlink(),
                    }
                    for path in sorted(target.rglob("*"))
                    if path.is_file()
                }
            cases.append({"operation": "copy", "variant": variant, "observed": files})
    with DESTINATION.open("x") as stream:
        json.dump({"source_sha256": SOURCE_SHA, "cases": cases}, stream, indent=2)
        stream.write("\n")
    print(f"Froze {len(cases)} preview reference cases")


if __name__ == "__main__":
    capture()
