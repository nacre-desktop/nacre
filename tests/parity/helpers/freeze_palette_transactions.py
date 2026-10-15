#!/usr/bin/env python3
"""Freeze current prepared commits/demo restore before caller cutover; no overwrites."""

import base64
import copy
import gzip
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from unittest.mock import patch

from freeze_palette_pipeline import MONITORS, NOW, ROOT, SHA, TOOLS, pooled, snapshot

DEST = (
    ROOT / "crates/nacre-helper/tests/fixtures/palette-transactions-reference.json.gz"
)


def expand(value, strings):
    if isinstance(value, dict):
        if set(value) == {"$string"}:
            return strings[value["$string"]]
        return {key: expand(child, strings) for key, child in value.items()}
    if isinstance(value, list):
        return [expand(child, strings) for child in value]
    return value


def capture():
    if DEST.exists():
        raise ValueError("Refusing to overwrite palette transaction references")
    for name, digest in SHA.items():
        if hashlib.sha256((TOOLS / name).read_bytes()).hexdigest() != digest:
            raise ValueError("Reference source changed: " + name)
    packed = json.loads(
        gzip.decompress(
            (DEST.parent / "palette-pipeline-reference.json.gz").read_bytes()
        )
    )
    original = expand(packed["data"], packed["strings"])
    initial = original["cases"][0]["inputs"]
    sys.path[:0] = [str(TOOLS), str(ROOT / "nacre/shell-cli/src")]
    original_spec, original_run = importlib.util.spec_from_file_location, subprocess.run
    rasters, cases = {}, []
    for name in (
        "prepared",
        "prepared-mode-mismatch",
        "prepared-allow-mode",
        "prepared-outside-cache",
        "prepared-hook",
        "restore",
        "restore-bad-preferences",
        "restore-missing-poster",
    ):
        with tempfile.TemporaryDirectory(
            prefix="nacre-palette-transaction-"
        ) as directory:
            home = Path(directory)
            for relative, entry in initial.items():
                path = home / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                raw = (
                    entry["text"].replace("/fixture/home", str(home)).encode()
                    if "text" in entry
                    else base64.b64decode(entry["base64"])
                )
                path.write_bytes(raw)
                path.chmod(entry["mode"])
            for filename in ("scene.png", "next.png"):
                (home / filename).write_bytes((home / "scene.png").read_bytes())
                os.utime(home / filename, ns=(1700000000123456789,) * 2)
            state = home / ".local/state/nacre"
            wallpaper = state / "wallpaper"
            wallpaper.mkdir(parents=True)
            thumb = home / ".cache/nacre/wallpapers/probe/thumbnail.jpg"
            thumb.parent.mkdir(parents=True)
            thumb.write_bytes(b"public thumbnail fixture")
            data = json.loads((state / "scheme.json").read_text())
            data.update(name="dynamic", flavour="default", variant="tonalspot")
            (state / "scheme.json").write_text(json.dumps(data))
            candidate = copy.deepcopy(data)
            if "mode" in name:
                candidate["mode"] = "light"
            image = home / "next.png"
            if name == "prepared-outside-cache":
                thumb = home / "outside-thumbnail.jpg"
                thumb.write_bytes(b"public thumbnail fixture")
            if name == "prepared-hook":
                (home / ".config/nacre/cli.json").write_text(
                    json.dumps({"wallpaper": {"postHook": "custom hook"}})
                )
            request = dict(
                operation="commit_prepared",
                wallpaper=str(image),
                data=candidate,
                thumbnail=str(thumb),
                allow_mode_change=name == "prepared-allow-mode",
            )
            if name.startswith("restore"):
                request = dict(
                    operation="restore_demo",
                    snapshot=dict(
                        scheme=data,
                        poster=str(home / "scene.png"),
                        media=dict(
                            path=str(home / "scene.png"),
                            poster=str(home / "scene.png"),
                            thumbnail=str(thumb),
                            name="Fixture scene",
                            appliedAtMs=123,
                        ),
                        preferences=dict(palettePreset="wallpaper", paletteMode="dark"),
                    ),
                )
                active = copy.deepcopy(data)
                active["mode"] = "light"
                (state / "scheme.json").write_text(json.dumps(active))
                prefs = home / ".config/nacre/wallpaper-picker.json"
                prefs.write_text(
                    json.dumps(
                        dict(
                            palettePreset="super-red",
                            paletteMode="light",
                            rotationMinutes=30,
                            unknown="preserve",
                        )
                    )
                )
                (wallpaper / "last.txt").write_text(str(image))
                (wallpaper / "path.txt").write_text(str(image))
                (wallpaper / "media.json").write_text(
                    json.dumps(
                        dict(path=str(image), poster=str(image), name="Current scene")
                    )
                )
                (wallpaper / "current").symlink_to(image)
                (wallpaper / "thumbnail.jpg").symlink_to(thumb)
                if name == "restore-bad-preferences":
                    request["snapshot"]["preferences"]["unknown"] = "reject"
                if name == "restore-missing-poster":
                    request["snapshot"]["poster"] = str(home / "missing.png")
            environment = {
                **os.environ,
                "HOME": str(home),
                "XDG_CONFIG_HOME": str(home / ".config"),
                "XDG_STATE_HOME": str(home / ".local/state"),
                "XDG_CACHE_HOME": str(home / ".cache"),
                "NACRE_LANGUAGE": "en",
                "NACRE_COMPOSITOR": "hyprland",
                "NACRE_HELPER_TOOLS": str(TOOLS),
                "NACRE_HELPER_BINARY": str(ROOT / "target/debug/nacre-helper"),
            }
            environment.pop("NACRE_PALETTE_LOCKED", None)

            def spec(name, path, *args, fixture_home=home, **kwargs):
                result = original_spec(name, path, *args, **kwargs)
                if name not in ("file_icons", "lock_config"):
                    return result
                execute = result.loader.exec_module

                def execute_fixture(loaded):
                    execute(loaded)
                    if name == "file_icons":
                        loaded.SYSTEM_ICONS = fixture_home / "empty-system-icons"
                    else:
                        loaded.monitor_layout = lambda requested=None: MONITORS

                result.loader.exec_module = execute_fixture
                return result

            def recording_run(command, *args, **kwargs):
                reply = original_run(command, *args, **kwargs)
                if Path(command[0]).name == "rsvg-convert":
                    key = (
                        hashlib.sha256(kwargs["input"]).hexdigest()
                        + ":"
                        + ":".join(command[1:])
                    )
                    rasters[key] = dict(
                        svg=kwargs["input"].decode(),
                        arguments=command[1:],
                        png=base64.b64encode(reply.stdout).decode(),
                    )
                return reply

            with (
                patch.dict(os.environ, environment, clear=True),
                patch("importlib.util.spec_from_file_location", side_effect=spec),
                patch("subprocess.run", side_effect=recording_run),
            ):
                import nacre_native_compositor as compositor

                compositor._close()
                compositor.backend_name = "hyprland"
                source = original_spec(
                    "transaction_reference", TOOLS / "classic-state.py"
                )
                module = importlib.util.module_from_spec(source)
                source.loader.exec_module(module)
                before = snapshot(home)
                error = None
                with patch.object(module.time, "time", return_value=NOW):
                    try:
                        if request["operation"] == "commit_prepared":
                            result = module.commit_prepared(
                                home,
                                Path(request["wallpaper"]),
                                request["data"],
                                Path(request["thumbnail"]),
                                live=False,
                                allow_mode_change=request["allow_mode_change"],
                            )
                        else:
                            result = module.restore_demo(
                                home, request["snapshot"], live=False
                            )
                    except ValueError as failure:
                        result = None
                        error = dict(type=type(failure).__name__, message=str(failure))
                after = snapshot(home)
                compositor._close()

            def normalize(value, fixture_home=home):
                return json.loads(
                    json.dumps(value).replace(str(fixture_home), "/fixture/home")
                )

            cases.append(
                dict(
                    name=name,
                    request=normalize(request),
                    result=normalize(result),
                    error=error,
                    inputs=before,
                    outputs=after,
                )
            )
    payload = dict(
        source_sha256=SHA,
        now_ms=int(NOW * 1000),
        monitors=MONITORS,
        input_mtime_ns=1700000000123456789,
        normalizations=original["normalizations"],
        rsvg=list(rasters.values()),
        cases=cases,
    )
    DEST.write_bytes(
        gzip.compress((json.dumps(pooled(payload), indent=2) + "\n").encode(), mtime=0)
    )
    print(
        f"Frozen {len(cases)} transaction cases; {DEST.stat().st_size} compressed bytes"
    )


if __name__ == "__main__":
    capture()
