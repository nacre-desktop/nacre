#!/usr/bin/env python3
"""Capture the unchanged Colors helper once, with complete file/result references."""

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
import time
from unittest.mock import patch
from PIL import Image, ImageFont

from colors_reference_files import Files, signature, unpack
from freeze_palette_pipeline import MONITORS, NOW, ROOT, SHA, TOOLS, pooled

DEST = ROOT / "crates/nacre-helper/tests/fixtures/colors-pipeline-reference.json.gz"


def capture():
    if DEST.exists():
        raise ValueError("Refusing to overwrite Colors references")
    if (
        hashlib.sha256((TOOLS / "colors.py").read_bytes()).hexdigest()
        != SHA["colors.py"]
    ):
        raise ValueError("Colors reference source changed")
    packed = json.loads(
        gzip.decompress(
            (DEST.parent / "palette-pipeline-reference.json.gz").read_bytes()
        )
    )
    initial = unpack(packed["data"], packed["strings"])["cases"][0]["inputs"]
    original_spec, original_run, original_font = (
        importlib.util.spec_from_file_location,
        subprocess.run,
        ImageFont.truetype,
    )
    sys.path[:0] = [str(TOOLS), str(ROOT / "nacre/shell-cli/src")]
    rasters, cases, key_vectors = {}, [], []
    for name in (
        "natural-cache",
        "pop-compare",
        "tide",
        "picked",
        "favorite-apply",
        "exports-card",
    ):
        with tempfile.TemporaryDirectory(prefix="nacre-colors-reference-") as directory:
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
            image = Image.new("RGB", (64, 40), "#172b45")
            image.paste("#db788f", (24, 12, 40, 28))
            image.save(home / "scene.png")
            os.utime(home / "scene.png", ns=(1700000000123456789,) * 2)
            library = home / "Pictures/Wallpapers"
            library.mkdir(parents=True)
            (library / "scene_copy.png").write_bytes((home / "scene.png").read_bytes())
            os.utime(library / "scene_copy.png", ns=(1700000000123456789,) * 2)
            request = dict(image=str(home / "scene.png"), hour=12)
            if name == "pop-compare":
                request.update(mode="light", personality="pop", compare=True)
            if name == "tide":
                request.update(
                    personality="tide",
                    hour=20,
                    autoMode=True,
                    timeTint=True,
                    brightness=0.15,
                    saturation=1.25,
                )
            if name == "picked":
                request.update(pick=dict(x=0.5, y=0.5))
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

            def spec(module_name, path, *args, fixture_home=home, **kwargs):
                result = original_spec(module_name, path, *args, **kwargs)
                if module_name not in ("file_icons", "lock_config"):
                    return result
                execute = result.loader.exec_module

                def execute_fixture(loaded):
                    execute(loaded)
                    if module_name == "file_icons":
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

            def card_font(font, *args, **kwargs):
                if font == "DejaVuSans.ttf":
                    font = str(DEST.parent / "lock-fonts/FixtureCard.ttf")
                return original_font(font, *args, **kwargs)

            with (
                patch.dict(os.environ, environment, clear=True),
                patch("importlib.util.spec_from_file_location", side_effect=spec),
                patch("subprocess.run", side_effect=recording_run),
                patch.object(ImageFont, "truetype", side_effect=card_font),
            ):
                import nacre_native_compositor as compositor

                compositor._close()
                compositor.backend_name = "hyprland"
                source = original_spec("colors_reference", TOOLS / "colors.py")
                colors = importlib.util.module_from_spec(source)
                source.loader.exec_module(colors)
                before = Files(home, []).snapshot()
                results = []
                steps = []
                signatures = [signature(home / "scene.png")]
                clock = time.struct_time((2026, 7, 14, 14, 30, 0, 1, 195, -1))

                def stage(request, oracle=colors):
                    result = oracle.stage(request)
                    record = oracle.record(result["id"])
                    favorite = next(
                        (
                            row
                            for row in oracle.stored()["favorites"]
                            if row["id"] == record["request"].get("favoriteId")
                        ),
                        None,
                    )
                    if not any(
                        vector["identity"] == result["id"] for vector in key_vectors
                    ):
                        key_vectors.append(
                            copy.deepcopy(
                                dict(
                                    identity=result["id"],
                                    engine=record["engine"],
                                    signature=record["signature"],
                                    request=record["request"],
                                    favorite=favorite,
                                )
                            )
                        )
                    return result

                with (
                    patch.object(colors.time, "time", return_value=NOW),
                    patch.object(colors.time, "localtime", return_value=clock),
                    patch.object(
                        colors.time, "strftime", return_value="20260714-143000"
                    ),
                ):
                    preview = stage(request)
                    results.append(preview)
                    steps.append(dict(action="preview", request=request))
                    identity = preview["id"]
                    if name == "natural-cache":
                        results.append(stage(request))
                        steps.append(dict(action="preview", request=request))
                    if name == "favorite-apply":
                        results.append(colors.favorite(identity, "Fixture palette"))
                        steps.append(
                            dict(action="favorite", id_from=0, name="Fixture palette")
                        )
                        favorite = colors.stored()["favorites"][0]["id"]
                        chosen = dict(
                            image=str(home / "scene.png"),
                            hour=12,
                            mode="light",
                            favoriteId=favorite,
                        )
                        picked = stage(chosen)
                        results.append(picked)
                        steps.append(dict(action="preview", request=chosen))
                        results.append(colors.apply(picked["id"], live=False))
                        steps.append(dict(action="apply", id_from=2))
                        results.append(colors.catalog())
                        steps.append(dict(action="catalog"))
                        with colors.store_lock():
                            values = colors.stored()
                            values["favorites"] = [
                                row
                                for row in values["favorites"]
                                if row["id"] != favorite
                            ]
                            colors.media.atomic(colors.STORE, values)
                        results.append(dict(deleted=True))
                        steps.append(dict(action="delete", id=favorite))
                    if name == "exports-card":
                        results.append(colors.export(identity))
                        steps.append(dict(action="export", id_from=0))
                        results.append(colors.card(identity))
                        steps.append(dict(action="card", id_from=0))
                compositor._close()
            files = Files(home, signatures)
            files.discover(results)
            cases.append(
                dict(
                    name=name,
                    steps=files.normalize(steps),
                    inputs=before,
                    results=files.normalize(results),
                    outputs=files.snapshot(),
                )
            )
    info = json.loads(
        (ROOT / "nacre/shell-cli/src/nacre_shell/engine-pin.json").read_text()
    )
    payload = dict(
        source_sha256=SHA["colors.py"],
        engine=info["engine"],
        clock=dict(now_ms=int(NOW * 1000), hour=14, export_date="20260714-143000"),
        monitors=MONITORS,
        input_mtime_ns=1700000000123456789,
        normalizations="Only disposable HOME roots, source stat dev/inode/ctime and their derived preview/cache names. Stat signatures are checked against real observed input metadata before replacement. Colors JSON byte format checked before normalization. All other bytes/modes/links/results exact. Pure cache-key vectors retain synthetic temporary input paths to verify exact hashes independently.",
        rsvg=list(rasters.values()),
        key_vectors=key_vectors,
        cases=cases,
    )
    DEST.write_bytes(
        gzip.compress((json.dumps(pooled(payload), indent=2) + "\n").encode(), mtime=0)
    )
    print(
        f"Frozen {len(cases)} Colors workflows, {len(key_vectors)} exact key vectors; {DEST.stat().st_size} compressed bytes"
    )


if __name__ == "__main__":
    capture()
