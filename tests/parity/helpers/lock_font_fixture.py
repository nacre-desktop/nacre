"""Explicit public font/provider inputs for original and native lock captures."""

import hashlib
import os
from pathlib import Path
import subprocess
from xml.sax.saxutils import escape


def fingerprints(base):
    result = {}
    for folder in (base / "fonts", base / "font-cache"):
        for path in sorted(folder.rglob("*")):
            if path.is_symlink():
                result[str(path.relative_to(base))] = ("link", str(path.readlink()))
            elif path.is_file():
                result[str(path.relative_to(base))] = (
                    hashlib.sha256(path.read_bytes()).hexdigest(),
                    path.stat().st_mtime_ns,
                    path.stat().st_mode & 0o777,
                )
    return result


def setup(base, home, reference=None):
    env = dict(
        os.environ,
        HOME=str(home),
        XDG_CONFIG_HOME=str(home / ".config"),
        XDG_DATA_HOME=str(home / ".local/share"),
        XDG_STATE_HOME=str(home / ".local/state"),
        XDG_CACHE_HOME=str(base / "chooser-cache"),
    )
    for key in ("FONTCONFIG_FILE", "FONTCONFIG_PATH", "FONTCONFIG_SYSROOT"):
        env.pop(key, None)
    fonts = base / "fonts"
    fonts.mkdir()
    records = []
    for query, name in (
        ("DejaVu Sans:style=Book", "DejaVuSans.ttf"),
        ("DejaVu Sans:style=Bold", "DejaVuSans-Bold.ttf"),
    ):
        source = Path(
            subprocess.check_output(
                ["fc-match", "--format=%{file}", query], env=env, text=True
            )
        )
        if not source.resolve().is_relative_to("/usr/share/fonts"):
            raise AssertionError(
                "The fixture requires public system DejaVu font inputs"
            )
        raw = source.read_bytes()
        (fonts / name).write_bytes(raw)
        records.append(
            dict(query=query, file=name, sha256=hashlib.sha256(raw).hexdigest())
        )
    if reference is not None and records != reference:
        raise AssertionError("Pinned public lock font inputs differ")
    config = base / "fontconfig.xml"
    config.write_text(
        "<fontconfig><reset-dirs/><dir>"
        + escape(str(fonts))
        + "</dir><cachedir>"
        + escape(str(base / "font-cache"))
        + "</cachedir></fontconfig>"
    )
    env.update(FONTCONFIG_FILE=str(config), XDG_CACHE_HOME=str(home / ".cache"))
    subprocess.run(
        ["fc-cache", "-f", str(fonts)], env=env, check=True, capture_output=True
    )
    return dict(FONTCONFIG_FILE=str(config)), records
