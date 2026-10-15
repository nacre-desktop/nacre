#!/usr/bin/env python3
"""Freeze current selection/theme workflows with public art and declared clocks."""

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

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "nacre/shell-tools/wallpaper-media.py"
DEST = (
    ROOT
    / "crates/nacre-helper/tests/fixtures/wallpaper-media-pipeline-reference.json.gz"
)
NOW = 1700000000.125
MTIME = 1700000000123456789
WORKER = """import importlib.util,json,sys,time
from pathlib import Path
source=Path(sys.argv[1]);sys.path.insert(0,str(source.parent))
spec=importlib.util.spec_from_file_location("original_media",source)
media=importlib.util.module_from_spec(spec);spec.loader.exec_module(media)
media.time.time=lambda:1700000000.125
home=Path.home();results=[]
results.append(media.select(str(home/"dark.png")))
results.append(media.theme({"paletteMode":"light"}))
results.append(media.theme({"palettePersonality":"harmony"}))
results.append(media.theme({"paletteAccent":"2255aa"}))
results.append(media.theme({"paletteAccent":"auto"}))
results.append(media.preference({"paletteAutoMode":True}))
results.append(media.select(str(home/"bright.png")))
results.append(media.select(str(home/"dark.png")))
results.append(media.catalog())
print(json.dumps(results))
"""
SVG_RECORDER = """#!/usr/bin/python3
import base64,fcntl,json,os,subprocess,sys
from pathlib import Path
raw=sys.stdin.buffer.read();reply=subprocess.run(["/usr/bin/rsvg-convert",*sys.argv[1:]],input=raw,capture_output=True)
if reply.returncode==0:
 with Path(os.environ["NACRE_MEDIA_SVG_LOG"]).open("a") as stream:
  fcntl.flock(stream,fcntl.LOCK_EX);stream.write(json.dumps(dict(svg=raw.decode(),arguments=sys.argv[1:],png=base64.b64encode(reply.stdout).decode()))+"\\n")
sys.stdout.buffer.write(reply.stdout);sys.stderr.buffer.write(reply.stderr);sys.exit(reply.returncode)
"""


def clocks(value):
    if isinstance(value, dict):
        return {
            key: "@PUBLICATION_MS@"
            if key in ("changedAtMs", "publishedAtMs")
            else clocks(item)
            for key, item in value.items()
        }
    if isinstance(value, list):
        return [clocks(item) for item in value]
    return value


def normalized_files(files):
    rows = files.snapshot()
    for name, entry in rows.items():
        if name == ".local/state/nacre/presentation.json" and "text" in entry:
            value = json.loads(entry["text"])
            for key in ("changedAtMs", "publishedAtMs"):
                number = value[key]
                token = json.dumps(key) + ": " + str(number)
                assert entry["text"].count(token) == 1
                entry["text"] = entry["text"].replace(
                    token, json.dumps(key) + ': "@PUBLICATION_MS@"'
                )
    return rows


def main():
    supplemental = "--cache-manifest" in sys.argv
    destination = (
        DEST.with_name("wallpaper-media-engine-cache-reference.json.gz")
        if supplemental
        else DEST
    )
    if destination.exists():
        raise ValueError("Refusing to replace media pipeline references")
    source_sha = hashlib.sha256(SOURCE.read_bytes()).hexdigest()
    if source_sha != "74795d256c7f4e44b5d19e6ed7b58a158d3a1905b552097c99de1681abcbc4a2":
        raise ValueError("Current media source changed")
    with tempfile.TemporaryDirectory(
        prefix="nacre-media-pipeline-original-"
    ) as directory:
        base = Path(directory)
        home = base / "home"
        home.mkdir()
        binary = base / "bin"
        binary.mkdir()
        recorder = binary / "rsvg-convert"
        recorder.write_text(SVG_RECORDER)
        recorder.chmod(0o700)
        for name, color in (("dark.png", "#172b45"), ("bright.png", "#eeeeee")):
            Image.new("RGB", (64, 40), color).save(home / name)
        bridge = home / ".local/share/nacre/shell/bin/nacre_shell"
        bridge.parent.mkdir(parents=True)
        bridge.write_bytes((ROOT / "nacre/shell-tools/cli-bridge.sh").read_bytes())
        bridge.chmod(0o755)
        config = home / ".config/nacre"
        config.mkdir(parents=True)
        (config / "cli.json").write_text('{"version":1,"wallpaper":{"postHook":null}}')
        (config / "wallpaper-picker.json").write_text(
            '{"version":1,"palettePreset":"wallpaper","paletteMode":"dark","palettePersonality":"natural"}'
        )
        state = home / ".local/state/nacre"
        state.mkdir(parents=True)
        scheme = json.loads(
            (ROOT / "nacre/shell-tools/reference-style.json").read_text()
        )
        scheme["name"] = "dynamic"
        scheme["version"] = 1
        (state / "scheme.json").write_text(json.dumps(scheme))
        monitors = [dict(name="", width=1920, height=1080)]
        cache = home / ".cache/nacre"
        ready = cache / "lock-ready"
        ready.mkdir(parents=True)
        (cache / "lock-outputs.json").write_text(json.dumps(monitors))
        key = hashlib.sha256(
            json.dumps(
                dict(name="", width=1920, height=1080, transform=None), sort_keys=True
            ).encode()
        ).hexdigest()[:12]
        Image.new("RGBA", (16, 16), (20, 30, 40, 255)).save(
            ready / f"dashboard-{key}-initial.png"
        )
        for path in home.rglob("*"):
            if path.is_file():
                os.utime(path, ns=(MTIME, MTIME))
        files = Files(home, [])
        inputs = files.snapshot()
        svg_log = base / "svg.jsonl"
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
            "NACRE_HELPER_BINARY": str(ROOT / "target/debug/nacre-helper"),
            "NACRE_HELPER_TOOLS": str(ROOT / "nacre/shell-tools"),
            "NACRE_LANGUAGE": "en",
            "NACRE_MEDIA_SVG_LOG": str(svg_log),
            "PATH": str(binary) + ":/usr/bin",
        }
        for key in (
            "NIRI_SOCKET",
            "HYPRLAND_INSTANCE_SIGNATURE",
            "NACRE_PRESENTATION_STREAM",
            "NACRE_PALETTE_LOCKED",
        ):
            env.pop(key, None)
        worker = subprocess.run(
            [sys.executable, "-c", WORKER, str(SOURCE)],
            env=env,
            capture_output=True,
            text=True,
            timeout=180,
            check=False,
        )
        if worker.returncode:
            Path("/tmp/nacre-media-pipeline-original-worker.log").write_text(
                worker.stderr
            )
            raise RuntimeError(
                "Original workflow failed; bounded details /tmp/nacre-media-pipeline-original-worker.log"
            )
        results = json.loads(worker.stdout)
        files.discover(results)
        svg = []
        if svg_log.exists():
            unique = {}
            for line in svg_log.read_text().splitlines():
                row = json.loads(line)
                unique[(row["svg"], tuple(row["arguments"]))] = row
            svg = list(unique.values())
        output = dict(
            source_sha256=source_sha,
            input_mtime_ns=MTIME,
            clock_seconds=NOW,
            inputs=inputs,
            results=clocks(files.normalize(results)),
            outputs=normalized_files(files),
            svg=svg,
            normalizations=[
                "disposable HOME/stat-derived preview and palette filenames",
                "only presentation changedAtMs/publishedAtMs are volatile; media appliedAtMs is a fixed observed Python clock",
            ],
        )
        if supplemental:
            records = {}
            for path in (home / ".cache/orient/palettes").glob("rust-*.json"):
                text = path.read_text().replace(str(home), "/fixture/home")
                value = json.loads(text)
                identity = json.dumps(
                    [value["source"]["path"], value["input"]], sort_keys=True
                )
                key = hashlib.sha256(identity.encode()).hexdigest()
                if key in records:
                    assert records[key]["text"] == text, (
                        "Conflicting original engine cache records"
                    )
                records[key] = dict(text=text, mode=path.stat().st_mode & 0o777)
            output = dict(
                source_sha256=source_sha,
                original_snapshot_sha256=hashlib.sha256(DEST.read_bytes()).hexdigest(),
                records=records,
                normalization="Only disposable HOME and stat-derived cache filenames; source path + input settings uniquely identify cache records",
            )
            destination.write_bytes(
                gzip.compress(json.dumps(output, indent=2).encode(), mtime=0)
            )
            print(
                f"Captured {len(records)} original engine cache records; original snapshot unchanged"
            )
            return
        destination.write_bytes(
            gzip.compress(json.dumps(output, indent=2).encode(), mtime=0)
        )
        print(
            f"Frozen {len(results)} selection/theme steps, {len(output['outputs'])} files, {len(svg)} exact SVG responses"
        )


if __name__ == "__main__":
    main()
