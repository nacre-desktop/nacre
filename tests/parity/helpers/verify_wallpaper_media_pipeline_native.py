#!/usr/bin/env python3
"""Replay the immutable Python selection/theme workflow in one native process."""

import base64
import gzip
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

from colors_reference_files import Files
from freeze_wallpaper_media_pipeline import clocks, normalized_files

from historical_palette_assets import stage as historical_assets

ROOT = Path(__file__).resolve().parents[3]


def main():
    reference = json.loads(
        gzip.decompress(
            (
                ROOT
                / "crates/nacre-helper/tests/fixtures/wallpaper-media-pipeline-reference.json.gz"
            ).read_bytes()
        )
    )
    with tempfile.TemporaryDirectory(
        prefix="nacre-media-pipeline-native-"
    ) as directory:
        base = Path(directory)
        home = base / "home"
        home.mkdir()
        binary = base / "bin"
        binary.mkdir()
        for name, entry in reference["inputs"].items():
            path = home / name
            path.parent.mkdir(parents=True, exist_ok=True)
            if "link" in entry:
                path.symlink_to(entry["link"].replace("/fixture/home", str(home)))
            else:
                raw = (
                    entry["text"].replace("/fixture/home", str(home)).encode()
                    if "text" in entry
                    else base64.b64decode(entry["base64"])
                )
                path.write_bytes(raw)
                path.chmod(entry["mode"])
                os.utime(path, ns=(reference["input_mtime_ns"],) * 2)
        renderer = binary / "rsvg-convert"
        renderer.write_bytes(
            (ROOT / "tests/parity/helpers/svg_fixture.py").read_bytes()
        )
        renderer.chmod(0o700)
        responses = base / "svg.json"
        responses.write_text(json.dumps(reference["svg"]))
        env = {
            **os.environ,
            "HOME": str(home),
            "XDG_CONFIG_HOME": str(home / ".config"),
            "XDG_STATE_HOME": str(home / ".local/state"),
            "XDG_CACHE_HOME": str(home / ".cache"),
            "XDG_RUNTIME_DIR": str(home / "runtime"),
            "DBUS_SESSION_BUS_ADDRESS": "unix:path=" + str(home / "absent-bus"),
            "DBUS_SYSTEM_BUS_ADDRESS": "unix:path=" + str(home / "absent-system-bus"),
            "NACRE_COMPOSITOR": "hyprland",
            "NACRE_HELPER_TOOLS": historical_assets(home.parent),
            "NACRE_LANGUAGE": "en",
            "NACRE_SVG_REFERENCE": str(responses),
            "PATH": str(binary) + ":/usr/bin",
        }
        for key in (
            "NIRI_SOCKET",
            "HYPRLAND_INSTANCE_SIGNATURE",
            "NACRE_PRESENTATION_STREAM",
            "NACRE_PALETTE_LOCKED",
        ):
            env.pop(key, None)
        provider_path = (
            ROOT
            / "crates/nacre-helper/tests/fixtures/wallpaper-media-icon-provider-reference.json"
        )
        provider = json.loads(provider_path.read_text())
        original_path = (
            ROOT
            / "crates/nacre-helper/tests/fixtures/wallpaper-media-pipeline-reference.json.gz"
        )
        assert (
            hashlib.sha256(original_path.read_bytes()).hexdigest()
            == provider["original_workflow_sha256"]
        )
        system_icons = base / "system-icons"
        for theme in ("Papirus", "Papirus-Dark"):
            path = system_icons / theme / "index.theme"
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("[Icon Theme]\nName=" + theme + "\n")
        for entry in provider["entries"].values():
            canonical = system_icons / entry["canonical"]
            canonical.parent.mkdir(parents=True, exist_ok=True)
            if not canonical.exists():
                canonical.write_text("<svg/>\n")
        for name, entry in provider["entries"].items():
            path = system_icons / name
            path.parent.mkdir(parents=True, exist_ok=True)
            if path != system_icons / entry["canonical"]:
                path.symlink_to(system_icons / entry["canonical"])
        steps = [
            dict(action="select", value=str(home / "dark.png")),
            *[
                dict(action="theme", value=value)
                for value in (
                    {"paletteMode": "light"},
                    {"palettePersonality": "harmony"},
                    {"paletteAccent": "2255aa"},
                    {"paletteAccent": "auto"},
                )
            ],
            dict(action="preferences", value={"paletteAutoMode": True}),
            dict(action="select", value=str(home / "bright.png")),
            dict(action="select", value=str(home / "dark.png")),
            dict(action="catalog"),
        ]
        result = subprocess.run(
            [str(ROOT / "target/debug/examples/wallpaper-media-parity")],
            input=json.dumps(
                dict(
                    now_ms=int(reference["clock_seconds"] * 1000),
                    steps=steps,
                    system_icons=str(system_icons),
                )
            ),
            env=env,
            capture_output=True,
            text=True,
            timeout=180,
        )
        if result.returncode:
            Path("/tmp/nacre-media-native-pipeline-worker.log").write_text(
                result.stderr
            )
            raise AssertionError(
                "Native media workflow failed; /tmp/nacre-media-native-pipeline-worker.log"
            )
        values = json.loads(result.stdout)
        files = Files(home, [])
        files.discover(values)
        actual = clocks(files.normalize(values))
        if actual != reference["results"]:
            Path("/tmp/nacre-media-native-pipeline-results.json").write_text(
                json.dumps(dict(actual=actual, expected=reference["results"]), indent=2)
            )
            raise AssertionError(
                "Native media results differ; bounded artifact /tmp/nacre-media-native-pipeline-results.json"
            )
        supplemental = json.loads(
            gzip.decompress(
                (
                    ROOT
                    / "crates/nacre-helper/tests/fixtures/wallpaper-media-engine-cache-reference.json.gz"
                ).read_bytes()
            )
        )
        original_path = (
            ROOT
            / "crates/nacre-helper/tests/fixtures/wallpaper-media-pipeline-reference.json.gz"
        )
        assert (
            hashlib.sha256(original_path.read_bytes()).hexdigest()
            == supplemental["original_snapshot_sha256"]
        ), "Original reference snapshot changed"

        def cache_key(text):
            value = json.loads(text)
            return hashlib.sha256(
                json.dumps(
                    [value["source"]["path"], value["input"]], sort_keys=True
                ).encode()
            ).hexdigest()

        observed_cache = {}
        for path in (home / ".cache/orient/palettes").glob("rust-*.json"):
            text = path.read_text().replace(str(home), "/fixture/home")
            key = cache_key(text)
            record = dict(text=text, mode=path.stat().st_mode & 0o777)
            assert key not in observed_cache or observed_cache[key] == record, (
                "Conflicting native engine cache records"
            )
            observed_cache[key] = record
        assert observed_cache == supplemental["records"], (
            "Full native engine cache differs from immutable original manifest"
        )
        for name, record in reference["outputs"].items():
            if name.startswith(".cache/orient/palettes/"):
                assert record == supplemental["records"][cache_key(record["text"])], (
                    "Original frozen cache record differs"
                )
        actual = {
            name: record
            for name, record in normalized_files(files).items()
            if not name.startswith(".cache/orient/palettes/")
        }
        expected = {
            name: record
            for name, record in reference["outputs"].items()
            if not name.startswith(".cache/orient/palettes/")
        }
        for record in actual.values():
            if "link" in record and record["link"].startswith(str(system_icons) + "/"):
                record["link"] = (
                    "/usr/share/icons" + record["link"][len(str(system_icons)) :]
                )
        extra = set(actual) - set(expected)
        missing = set(expected) - set(actual)
        assert not extra and not missing, (
            f"Native media file set differs: extra{sorted(extra)[:4]},missing{sorted(missing)[:4]}"
        )
        for name in expected:
            if actual[name] != expected[name]:
                Path(
                    "/tmp/nacre-media-native-pipeline-file-difference.json"
                ).write_text(
                    json.dumps(
                        dict(path=name, actual=actual[name], expected=expected[name]),
                        indent=2,
                    )
                )
                raise AssertionError(
                    f"Native media file differs: {name}; bounded artifact /tmp/nacre-media-native-pipeline-file-difference.json"
                )
        print(
            f"Native media pipeline: {len(values)} results, {len(actual)} non-engine snapshots and all {len(observed_cache)} engine cache records exact"
        )


if __name__ == "__main__":
    main()
