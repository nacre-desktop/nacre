#!/usr/bin/env python3
"""Freeze current full publication with explicit external-dependency fixtures.

Never overwrite an existing reference. Warm lock panels test publication without
recapturing machine fonts or account/uptime data; cold rendering has separate specs.
"""

import base64
import hashlib
import gzip
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from unittest.mock import patch
from PIL import Image

ROOT = Path(__file__).resolve().parents[3]
TOOLS = ROOT / "nacre/shell-tools"
DEST = ROOT / "crates/nacre-helper/tests/fixtures/palette-pipeline-reference.json.gz"
SHA = {
    "classic-state.py": "4929405246261845685ee2a4b33bf306a31fc563dc72b8fbf9c722e750c0da0d",
    "colors.py": "f98c1f750242503b39c75d8c4f343d3b58e45a7ce549dc8c2839f2c23b514c53",
    "palette_engine.py": "20e21711f3d1641b43e72d92cdc2be5847edb9ba8dceb4ca219352ae61d533b7",
    "branding.py": "6fb56586038644fddbe1f06036e720eed59346eb6aba5b5f039aff052cb36bb8",
    "lock-config.py": "7a70069ddacfbebf5954f17bad4b7bf42023347871b17e802cd5b267b85f0c3b",
}
MONITORS = [dict(name="fixture-output", width=1920, height=1080, transform=0)]
NOW = 1780000000.125


def pooled(value):
    """Store repeated long strings once, preserving every frozen byte on expansion."""
    strings, indices = [], {}

    def pack(item):
        if isinstance(item, str) and len(item) > 256:
            if item not in indices:
                indices[item] = len(strings)
                strings.append(item)
            return {"$string": indices[item]}
        if isinstance(item, dict):
            return {key: pack(child) for key, child in item.items()}
        if isinstance(item, list):
            return [pack(child) for child in item]
        return item

    data = pack(value)
    return dict(
        encoding="pooled-strings-v1",
        payload_sha256=hashlib.sha256(
            (json.dumps(value, indent=2) + "\n").encode()
        ).hexdigest(),
        strings=strings,
        data=data,
    )


def snapshot(home):
    rows = {}
    aliases = {}
    for path in home.rglob("*.png"):
        if path.parent.name in ("branding", "login-images") and len(path.stem) == 64:
            aliases[path.stem] = (
                "@" + hashlib.sha256(path.read_bytes()).hexdigest() + "@"
            )

    def normalized(text):
        text = text.replace(str(home), "/fixture/home")
        for old, new in aliases.items():
            text = text.replace(old, new)
        return text

    for path in sorted(home.rglob("*")):
        relative = normalized(str(path.relative_to(home)))
        if path.is_symlink():
            rows[relative] = dict(link=normalized(os.readlink(path)))
        elif path.is_file():
            raw = path.read_bytes()
            try:
                content = dict(text=normalized(raw.decode()))
            except UnicodeDecodeError:
                content = dict(base64=base64.b64encode(raw).decode())
            rows[relative] = dict(mode=path.stat().st_mode & 0o777, **content)
    return rows


def capture():
    if DEST.exists():
        raise ValueError("Refusing to overwrite frozen palette pipeline outputs")
    for name, digest in SHA.items():
        if hashlib.sha256((TOOLS / name).read_bytes()).hexdigest() != digest:
            raise ValueError("Reference source changed: " + name)
    sys.path[:0] = [str(TOOLS), str(ROOT / "nacre/shell-cli/src")]
    original_spec = importlib.util.spec_from_file_location
    original_run = subprocess.run
    rasters = {}
    cases = []

    def recording_run(command, *args, **kwargs):
        reply = original_run(command, *args, **kwargs)
        if Path(command[0]).name == "rsvg-convert":
            raw = kwargs["input"]
            key = hashlib.sha256(raw).hexdigest() + ":" + ":".join(command[1:])
            rasters[key] = dict(
                svg=raw.decode(),
                arguments=command[1:],
                png=base64.b64encode(reply.stdout).decode(),
            )
        return reply

    for backend in ("hyprland", "niri"):
        for mode in ("dark", "light"):
            for fixed in (False, True):
                with tempfile.TemporaryDirectory(
                    prefix="nacre-palette-reference-"
                ) as directory:
                    home = Path(directory)
                    environment = {
                        **os.environ,
                        "HOME": str(home),
                        "XDG_CONFIG_HOME": str(home / ".config"),
                        "XDG_STATE_HOME": str(home / ".local/state"),
                        "XDG_CACHE_HOME": str(home / ".cache"),
                        "NACRE_LANGUAGE": "en",
                        "NACRE_COMPOSITOR": backend,
                        "NACRE_HELPER_TOOLS": str(TOOLS),
                        "NACRE_HELPER_BINARY": str(
                            ROOT / "target/release/nacre-helper"
                        ),
                        "NIRI_SOCKET": str(home / "absent-niri"),
                        "HYPRLAND_INSTANCE_SIGNATURE": "absent-fixture",
                        "XDG_RUNTIME_DIR": str(home / "runtime"),
                        "DBUS_SESSION_BUS_ADDRESS": "unix:path="
                        + str(home / "absent-dbus"),
                    }
                    for key in ("NACRE_PRESENTATION_STREAM", "NACRE_PALETTE_LOCKED"):
                        environment.pop(key, None)

                    def spec(name, path, *args, fixture_home=home, **kwargs):
                        result = original_spec(name, path, *args, **kwargs)
                        if name not in ("file_icons", "lock_config"):
                            return result
                        execute = result.loader.exec_module

                        def execute_fixture(loaded):
                            execute(loaded)
                            if name == "file_icons":
                                loaded.SYSTEM_ICONS = (
                                    fixture_home / "empty-system-icons"
                                )
                            else:
                                loaded.monitor_layout = lambda requested=None: MONITORS

                        result.loader.exec_module = execute_fixture
                        return result

                    with (
                        patch.dict(os.environ, environment, clear=True),
                        patch(
                            "importlib.util.spec_from_file_location", side_effect=spec
                        ),
                        patch("subprocess.run", side_effect=recording_run),
                    ):
                        import nacre_native_compositor as compositor

                        compositor._close()
                        compositor.backend_name = backend
                        source = original_spec(
                            "palette_reference", TOOLS / "classic-state.py"
                        )
                        module = importlib.util.module_from_spec(source)
                        source.loader.exec_module(module)
                        image = home / "scene.png"
                        Image.new("RGB", (37, 23), "#294856").save(image)
                        os.utime(image, ns=(1700000000123456789, 1700000000123456789))
                        scheme = json.loads(
                            (TOOLS / "reference-style.json").read_text()
                        )
                        scheme["mode"] = mode
                        state = home / ".local/state/nacre"
                        state.mkdir(parents=True)
                        (state / "scheme.json").write_text(json.dumps(scheme))
                        config = home / ".config/nacre"
                        config.mkdir(parents=True)
                        (config / "wallpaper-picker.json").write_text(
                            json.dumps(
                                dict(
                                    palettePreset="super-red" if fixed else "wallpaper",
                                    paletteMode=mode,
                                    unknown="preserve",
                                )
                            )
                        )
                        (config / "desktop.json").write_text(
                            json.dumps(dict(lockMedia=False, lockWeather=True))
                        )
                        for version in ("3.0", "4.0"):
                            folder = home / (".config/gtk-" + version)
                            folder.mkdir(parents=True)
                            (folder / "settings.ini").write_text(
                                "[Settings]\ngtk-theme-name=Before\ngtk-application-prefer-dark-theme=0\ngtk-icon-theme-name=Before\nkeep=original\n"
                            )
                        for version in (5, 6):
                            folder = home / f".config/qt{version}ct"
                            folder.mkdir(parents=True)
                            (folder / f"qt{version}ct.conf").write_text(
                                "[Appearance]\ncolor_scheme_path=before\ncustom_palette=false\nicon_theme=Before\nkeep=original\n"
                            )
                        for base in ("Papirus", "Papirus-Dark"):
                            folder = home / ".local/share/icons" / base
                            folder.mkdir(parents=True)
                            (folder / "index.theme").write_text(
                                "[Icon Theme]\nName=Fixture\n"
                            )
                        for size in (32, 48, 64):
                            folder = (
                                home
                                / f".local/share/icons/Papirus/{size}x{size}/places"
                            )
                            folder.mkdir(parents=True)
                            for family in ("teal", "red", "blue", "grey"):
                                (folder / f"folder-{family}.svg").write_text(
                                    '<svg xmlns="http://www.w3.org/2000/svg"/>'
                                )
                        lockspec = original_spec(
                            "reference_lock", TOOLS / "lock-config.py"
                        )
                        lock = importlib.util.module_from_spec(lockspec)
                        lockspec.loader.exec_module(lock)
                        ready = home / ".cache/nacre/lock-ready"
                        ready.mkdir(parents=True)
                        buffer = io.BytesIO()
                        Image.new("RGBA", (2880, 1620), (20, 30, 40, 255)).save(
                            buffer, format="PNG"
                        )
                        (
                            ready
                            / (
                                "dashboard-"
                                + lock.panel_key(MONITORS[0])
                                + "-initial.png"
                            )
                        ).write_bytes(buffer.getvalue())
                        (ready / "dashboard.json").write_text(
                            json.dumps(
                                dict(
                                    preferences=dict(lockMedia=False, lockWeather=True)
                                )
                            )
                        )
                        before = snapshot(home)
                        with patch.object(module.time, "time", return_value=NOW):
                            module.apply_palette(home, str(image), live=False)
                        after = snapshot(home)
                        cases.append(
                            dict(
                                name=f"{backend}-{mode}-{'fixed' if fixed else 'wallpaper'}",
                                backend=backend,
                                mode=mode,
                                fixed=fixed,
                                inputs=before,
                                outputs=after,
                            )
                        )
                        compositor._close()
    DEST.write_bytes(
        gzip.compress(
            (
                json.dumps(
                    pooled(
                        dict(
                            source_sha256=SHA,
                            now_ms=int(NOW * 1000),
                            monitors=MONITORS,
                            normalizations="Disposable HOME roots; cache PNG filenames derived from disposable paths or renderer stat are represented by their exact PNG SHA256, with source formulas independently checked. PNG bytes, all other files, modes, links and fields exact. Warm lock panels are inputs; cold rendering has separate tests.",
                            rsvg=list(rasters.values()),
                            cases=cases,
                        )
                    ),
                    indent=2,
                )
                + "\n"
            ).encode(),
            mtime=0,
        )
    )
    print(
        f"Frozen {len(cases)} complete publication cases, {len(rasters)} external SVG render responses"
    )


if __name__ == "__main__":
    capture()
