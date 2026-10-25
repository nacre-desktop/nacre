#!/usr/bin/env python3
"""Freeze warm preparation/cache repair with a complete, collision-safe file list."""

import base64
import gzip
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

from PIL import Image
from colors_reference_files import Files
from freeze_wallpaper_media_pipeline import SVG_RECORDER

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "nacre/shell-tools/wallpaper-media.py"
DEST = ROOT / "crates/nacre-helper/tests/fixtures/wallpaper-warm-reference.json.gz"
MTIME = 1700000000123456789
WORKER = """import importlib.util,json,sys
from pathlib import Path
source=Path(sys.argv[1]);sys.path.insert(0,str(source.parent))
spec=importlib.util.spec_from_file_location("original_media",source);media=importlib.util.module_from_spec(spec);spec.loader.exec_module(media)
scene=media.describe(Path.home()/"scene.png");media.atomic(media.STATE/"media.json",dict(scene,appliedAtMs=1700000000125));results=[scene]
results.append(media.warm(include_demo=False));results.append(media.warm(include_demo=False))
for path in (Path.home()/".cache/nacre/branding").glob("*.png"):path.unlink()
results.append(media.warm(include_demo=False));print(json.dumps(results))
"""


def file_list(home, files):
    rows = []
    for path in home.rglob("*"):
        if path == home / ".cache/nacre/native-engine-identity.json":
            files.validate_native_engine_identity_cache(path)
            continue
        name = files.normalize(str(path.relative_to(home)))
        if path.is_symlink():
            record = dict(link=files.normalize(str(path.readlink())))
        elif path.is_file():
            raw = path.read_bytes()
            try:
                record = dict(text=files.normalize(raw.decode()))
            except UnicodeDecodeError:
                record = dict(base64=base64.b64encode(raw).decode())
            record["mode"] = path.stat().st_mode & 0o777
        else:
            continue
        rows.append(dict(name=name, record=record))
    return sorted(rows, key=lambda value: json.dumps(value, sort_keys=True))


def add_markers(home, files):
    for path in (home / ".cache/nacre/wallpaper-media").glob("*.palette.json"):
        value = json.loads(path.read_text())
        identity = json.dumps(
            [files.normalize(value["source"]["path"]), value["input"]], sort_keys=True
        )
        files.aliases[path.name] = (
            "@WARM_"
            + hashlib.sha256(identity.encode()).hexdigest()[:24]
            + "@.palette.json"
        )


def main():
    if DEST.exists():
        raise ValueError("Refusing to replace warm references")
    digest = hashlib.sha256(SOURCE.read_bytes()).hexdigest()
    if digest != "74795d256c7f4e44b5d19e6ed7b58a158d3a1905b552097c99de1681abcbc4a2":
        raise ValueError("Current media source changed")
    with tempfile.TemporaryDirectory(prefix="nacre-warm-original-") as directory:
        base = Path(directory)
        home = base / "home"
        home.mkdir()
        binary = base / "bin"
        binary.mkdir()
        renderer = binary / "rsvg-convert"
        renderer.write_text(SVG_RECORDER)
        renderer.chmod(0o700)
        Image.new("RGB", (64, 40), "#172b45").save(home / "scene.png")
        (home / "Pictures/Wallpapers").mkdir(parents=True)
        Image.new("RGB", (64, 40), "#db788f").save(
            home / "Pictures/Wallpapers/library.png"
        )
        config = home / ".config/nacre"
        config.mkdir(parents=True)
        (config / "cli.json").write_text('{"version":1,"wallpaper":{"postHook":null}}')
        bridge = home / ".local/share/nacre/shell/bin/nacre_shell"
        bridge.parent.mkdir(parents=True)
        bridge.write_bytes((ROOT / "nacre/shell-tools/cli-bridge.sh").read_bytes())
        bridge.chmod(0o755)
        for path in home.rglob("*"):
            if path.is_file():
                os.utime(path, ns=(MTIME, MTIME))
        files = Files(home, [])
        inputs = files.snapshot()
        log = base / "svg.jsonl"
        env = {
            **os.environ,
            "HOME": str(home),
            "XDG_CONFIG_HOME": str(home / ".config"),
            "XDG_STATE_HOME": str(home / ".local/state"),
            "XDG_CACHE_HOME": str(home / ".cache"),
            "NACRE_HELPER_BINARY": str(ROOT / "target/debug/nacre-helper"),
            "NACRE_HELPER_TOOLS": str(ROOT / "nacre/shell-tools"),
            "NACRE_LANGUAGE": "en",
            "NACRE_MEDIA_SVG_LOG": str(log),
            "PATH": str(binary) + ":/usr/bin",
        }
        result = subprocess.run(
            [sys.executable, "-c", WORKER, str(SOURCE)],
            env=env,
            capture_output=True,
            text=True,
            timeout=180,
            check=True,
        )
        values = json.loads(result.stdout)
        files.discover(values)
        add_markers(home, files)
        svg = []
        if log.exists():
            unique = {}
            for line in log.read_text().splitlines():
                row = json.loads(line)
                unique[(row["svg"], tuple(row["arguments"]))] = row
            svg = list(unique.values())
        output = dict(
            source_sha256=digest,
            input_mtime_ns=MTIME,
            inputs=inputs,
            results=files.normalize(values),
            outputs=file_list(home, files),
            svg=svg,
        )
        DEST.write_bytes(gzip.compress(json.dumps(output, indent=2).encode(), mtime=0))
        print(
            f"Frozen {len(values) - 1} warm/repair stages, all {len(output['outputs'])} files without alias collisions"
        )


if __name__ == "__main__":
    main()
