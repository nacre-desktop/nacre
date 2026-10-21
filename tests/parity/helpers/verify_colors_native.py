#!/usr/bin/env python3
"""Replay immutable Colors workflows through the actual native implementation."""

import base64
import gzip
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

from colors_reference_files import Files, signature, unpack
from historical_palette_assets import stage as historical_assets

ROOT = Path(__file__).resolve().parents[3]
FIXTURE = ROOT / "crates/nacre-helper/tests/fixtures/colors-pipeline-reference.json.gz"


def compatible_picker_record(name, observed, frozen):
    """Main added one false default; preserve every other frozen file byte."""
    if (
        name != ".config/nacre/wallpaper-picker.json"
        or "text" not in observed
        or "text" not in frozen
    ):
        return observed
    current, original = json.loads(observed["text"]), json.loads(frozen["text"])
    if "paletteAutoMode" in original or current.get("paletteAutoMode") is not False:
        return observed
    current.pop("paletteAutoMode")
    token = ', "paletteAutoMode": false'
    if current != original or observed["text"].count(token) != 1:
        return observed
    return {**observed, "text": observed["text"].replace(token, "", 1)}


def main():
    packed = json.loads(gzip.decompress(FIXTURE.read_bytes()))
    reference = unpack(packed["data"], packed["strings"])
    binary = Path(
        os.environ.get(
            "NACRE_COLORS_PARITY", str(ROOT / "target/debug/examples/colors-parity")
        )
    )
    with tempfile.TemporaryDirectory(
        prefix="nacre-colors-engine-cache-"
    ) as engine_home:
        native_roots = {}
        for index, case in enumerate(reference["cases"]):
            with tempfile.TemporaryDirectory(
                prefix="nacre-colors-native-"
            ) as directory:
                base = Path(directory)
                home = base / "home"
                home.mkdir()
                tools = base / "bin"
                tools.mkdir()

                def replace(value, fixture_home=home):
                    return json.loads(
                        json.dumps(value).replace("/fixture/home", str(fixture_home))
                    )

                for relative, entry in case["inputs"].items():
                    path = home / relative
                    path.parent.mkdir(parents=True, exist_ok=True)
                    if "link" in entry:
                        path.symlink_to(
                            entry["link"].replace("/fixture/home", str(home))
                        )
                    else:
                        raw = (
                            entry["text"].replace("/fixture/home", str(home)).encode()
                            if "text" in entry
                            else base64.b64decode(entry["base64"])
                        )
                        path.write_bytes(raw)
                        path.chmod(entry["mode"])
                for path in (
                    home / "scene.png",
                    home / "Pictures/Wallpapers/scene_copy.png",
                ):
                    os.utime(path, ns=(reference["input_mtime_ns"],) * 2)
                renderer = tools / "rsvg-convert"
                renderer.write_bytes(
                    (ROOT / "tests/parity/helpers/svg_fixture.py").read_bytes()
                )
                renderer.chmod(0o755)
                targets = case["outputs"].get(".local/state/nacre/theme-targets.json")
                if targets:
                    for target in json.loads(targets["text"])["targets"]:
                        if target["available"] and target.get("tool"):
                            path = tools / target["tool"]
                            path.write_text(
                                "#!/bin/sh\necho 'Unexpected desktop tool action' >&2\nexit 99\n"
                            )
                            path.chmod(0o755)
                responses = base / "svg.json"
                responses.write_text(json.dumps(reference["rsvg"]))
                request = dict(
                    steps=replace(case["steps"]),
                    clock=reference["clock"],
                    monitors=reference["monitors"],
                    card_font=str(
                        ROOT
                        / "crates/nacre-helper/tests/fixtures/lock-fonts/FixtureCard.ttf"
                    ),
                )
                native_roots[str(home)] = case["name"]
                if index > 0:
                    request["engine_cache"] = str(Path(engine_home) / ".cache")
                env = dict(
                    HOME=str(home),
                    PATH=str(tools),
                    NACRE_HELPER_TOOLS=historical_assets(base),
                    XDG_DATA_DIRS=str(base / "empty-system-data"),
                    NACRE_SVG_REFERENCE=str(responses),
                    NACRE_COMPOSITOR="hyprland",
                    XDG_RUNTIME_DIR=str(base / "runtime"),
                    HYPRLAND_INSTANCE_SIGNATURE="absent-fixture",
                    NIRI_SOCKET=str(base / "absent-niri"),
                )
                worker = subprocess.run(
                    [str(binary), json.dumps(request)],
                    env=env,
                    text=True,
                    capture_output=True,
                    timeout=120,
                )
                if worker.returncode:
                    raise AssertionError(
                        f"{case['name']}: native exit{worker.returncode}: {worker.stderr[-1000:]}"
                    )
                if worker.stderr:
                    raise AssertionError(
                        f"{case['name']}: unexpected journal output: {worker.stderr[-1000:]}"
                    )
                results = json.loads(worker.stdout)
                files = Files(home, [signature(home / "scene.png")])
                files.discover(results)
                actual = files.normalize(results)
                if actual != case["results"]:
                    artifact = Path("/tmp") / (
                        "nacre-colors-" + case["name"] + "-difference.json"
                    )
                    artifact.write_text(
                        json.dumps(
                            dict(actual=actual, expected=case["results"]), indent=2
                        )
                    )
                    raise AssertionError(
                        f"{case['name']}: result mismatch; bounded diagnostics can read {artifact}"
                    )
                actual = files.snapshot()
                expected = case["outputs"]
                if set(actual) != set(expected):
                    raise AssertionError(
                        f"{case['name']}: extra files{sorted(set(actual) - set(expected))[:6]}; missing{sorted(set(expected) - set(actual))[:6]}"
                    )
                additive_defaults = 0
                for name in expected:
                    comparable = compatible_picker_record(
                        name, actual[name], expected[name]
                    )
                    additive_defaults += comparable != actual[name]
                    if comparable != expected[name]:
                        artifact = Path("/tmp") / (
                            "nacre-colors-" + case["name"] + "-file-difference.json"
                        )
                        artifact.write_text(
                            json.dumps(
                                dict(
                                    path=name,
                                    actual=actual[name],
                                    expected=expected[name],
                                ),
                                indent=2,
                            )
                        )
                        raise AssertionError(
                            f"{case['name']}: file mismatch {name}; details {artifact}"
                        )
                detail = (
                    "; one additive main picker default" if additive_defaults else ""
                )
                print(
                    f"{case['name']}: results and {len(expected)} files/links checked{detail}"
                )
        expected = json.loads(
            gzip.decompress(
                (FIXTURE.parent / "colors-engine-cache-reference.json.gz").read_bytes()
            )
        )["files"]
        actual = {}
        for path in sorted(
            (Path(engine_home) / ".cache/orient/palettes").glob("rust-*.json")
        ):
            text = path.read_text()
            data = json.loads(text)
            label = native_roots[str(Path(data["source"]["path"]).parent)]
            identity = hashlib.sha256(
                json.dumps(data["input"], sort_keys=True).encode()
            ).hexdigest()[:24]
            for root, name in native_roots.items():
                text = text.replace(root, "/fixture/" + name + "/home")
            actual[label + "/@PALETTE_" + identity + "@.json"] = dict(
                text=text, mode=path.stat().st_mode & 0o777
            )
        if actual != expected:
            artifact = Path("/tmp/nacre-colors-shared-cache-difference.json")
            artifact.write_text(
                json.dumps(dict(actual=actual, expected=expected), indent=2)
            )
            raise AssertionError(f"Shared engine cache differs; details {artifact}")
        print(f"Original shared engine cache: {len(expected)} exact files")
    print(f"Native Colors: {len(reference['cases'])} immutable workflows passed")


if __name__ == "__main__":
    main()
