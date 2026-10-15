#!/usr/bin/env python3
"""Freeze current media CLI/protocol inputs, outputs and files before porting."""

import base64
import gzip
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "nacre/shell-tools/wallpaper-media.py"
DEST = ROOT / "crates/nacre-helper/tests/fixtures/wallpaper-media-cli-reference.json.gz"


def snapshot(home):
    rows = {}
    for path in sorted(home.rglob("*")):
        name = str(path.relative_to(home))
        if path.is_symlink():
            rows[name] = dict(
                link=str(path.readlink()).replace(str(home), "/fixture/home")
            )
        elif path.is_dir():
            rows[name] = dict(directory=True, mode=path.stat().st_mode & 0o777)
        else:
            raw = path.read_bytes()
            try:
                data = dict(text=raw.decode().replace(str(home), "/fixture/home"))
            except UnicodeDecodeError:
                data = dict(base64=base64.b64encode(raw).decode())
            rows[name] = dict(mode=path.stat().st_mode & 0o777, **data)
    return rows


def main():
    if DEST.exists():
        raise ValueError("Refusing to overwrite media CLI references")
    expected_sha = "74795d256c7f4e44b5d19e6ed7b58a158d3a1905b552097c99de1681abcbc4a2"
    if hashlib.sha256(SOURCE.read_bytes()).hexdigest() != expected_sha:
        raise ValueError("Media source differs from the current Python reference")
    cases = []
    specifications = [
        ("help", ["--help"], ""),
        ("missing-action", [], ""),
        ("unknown-action", ["unknown"], ""),
        ("invalid-preference", ["preferences"], '{"layout":"invalid"}\n'),
        (
            "preferences",
            ["preferences"],
            '{"layout":"hexagons","paletteAutoMode":true}\n',
        ),
        ("missing-select", ["select", "{home}/missing.png"], ""),
        ("invalid-theme", ["theme"], '{"unknown":true}\n'),
        ("theme-without-wallpaper", ["theme"], '{"paletteMode":"light"}\n'),
        ("invalid-demo-token", ["demo-restore", "invalid"], ""),
        ("expired-demo-token", ["demo-restore", "0" * 32], ""),
        ("empty-import", ["import"], "[]\n"),
        ("invalid-import", ["import"], "{}\n"),
        (
            "serial-protocol",
            ["serve"],
            "\n".join(
                [
                    '{"action":"preferences","value":{"layout":"hexagons"}}',
                    '{"action":"preferences","value":{"paused":true}}',
                    '{"action":"unknown","value":{}}',
                    "broken JSON",
                    '{"action":"select","value":42}',
                    '{"action":"preferences","value":{"paletteAutoMode":true}}',
                ]
            )
            + "\n",
        ),
    ]
    for name, words, payload in specifications:
        with tempfile.TemporaryDirectory(
            prefix="nacre-media-cli-original-"
        ) as directory:
            home = Path(directory) / "home"
            home.mkdir(mode=0o700)
            env = {
                **os.environ,
                "HOME": str(home),
                "XDG_CONFIG_HOME": str(home / ".config"),
                "XDG_STATE_HOME": str(home / ".local/state"),
                "XDG_CACHE_HOME": str(home / ".cache"),
                "XDG_RUNTIME_DIR": str(home / "runtime"),
                "DBUS_SESSION_BUS_ADDRESS": "unix:path=" + str(home / "absent-bus"),
                "DBUS_SYSTEM_BUS_ADDRESS": "unix:path="
                + str(home / "absent-system-bus"),
                "NACRE_COMPOSITOR": "hyprland",
                "NACRE_LANGUAGE": "en",
                "NACRE_HELPER_TOOLS": str(ROOT / "nacre/shell-tools"),
                "NACRE_HELPER_BINARY": str(ROOT / "target/release/nacre-helper"),
            }
            for key in (
                "NIRI_SOCKET",
                "HYPRLAND_INSTANCE_SIGNATURE",
                "NACRE_PRESENTATION_STREAM",
                "NACRE_PALETTE_LOCKED",
            ):
                env.pop(key, None)
            before = snapshot(home)
            args = [word.replace("{home}", str(home)) for word in words]
            reply = subprocess.run(
                [sys.executable, str(SOURCE), *args],
                env=env,
                input=payload,
                capture_output=True,
                text=True,
                timeout=60,
            )
            cases.append(
                dict(
                    name=name,
                    args=words,
                    input=payload,
                    before=before,
                    stdout=reply.stdout.replace(str(home), "/fixture/home"),
                    stderr=reply.stderr.replace(str(home), "/fixture/home"),
                    exit=reply.returncode,
                    after=snapshot(home),
                )
            )
    DEST.write_bytes(
        gzip.compress(
            json.dumps(
                dict(source_sha256=expected_sha, cases=cases), indent=2
            ).encode(),
            mtime=0,
        )
    )
    print(
        f"Frozen {len(cases)} media CLI/protocol scenarios; all files and exits included"
    )


if __name__ == "__main__":
    main()
