#!/usr/bin/env python3
"""Install a checksum-pinned official Obsidian AppImage for this user only."""

import hashlib
from pathlib import Path
import platform
import subprocess
import tempfile
import urllib.request

VERSION = "1.13.7"
SHA256 = "e0d8e0a611624de8c9c7dcd8a9e648279fb0a0d552faa1312b7e4f3a5fa72663"
URL = f"https://github.com/obsidianmd/obsidian-releases/releases/download/v{VERSION}/Obsidian-{VERSION}.AppImage"


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def extract_verified(image, root):
    target = root / "squashfs-root"
    marker = target / ".verified-extraction-sha256"
    if target.exists():
        if marker.is_file() and marker.read_text().strip() == SHA256:
            return
        raise SystemExit(
            "Existing Obsidian extraction is unverified; left untouched for inspection"
        )
    with tempfile.TemporaryDirectory(prefix=".extract-", dir=root) as temporary:
        subprocess.run(
            [str(image), "--appimage-extract"],
            cwd=temporary,
            stdout=subprocess.DEVNULL,
            check=True,
        )
        extracted = Path(temporary) / "squashfs-root"
        for name in ("AppRun", "obsidian", "resources/obsidian.asar"):
            if not (extracted / name).is_file():
                raise SystemExit(
                    "Official extraction is missing required runtime files"
                )
        (extracted / marker.name).write_text(SHA256 + "\n")
        extracted.rename(target)


def main():
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise SystemExit("This pinned installer targets Linux x86_64")
    root = Path.home() / ".local/share/nacre/ai/obsidian" / VERSION
    root.mkdir(parents=True, exist_ok=True)
    image = root / "Obsidian.AppImage"
    if not image.exists():
        pending = root / "Obsidian.AppImage.partial"
        urllib.request.urlretrieve(URL, pending)
        if digest(pending) != SHA256:
            pending.unlink()
            raise SystemExit("Official asset checksum mismatch")
        pending.chmod(0o755)
        pending.rename(image)
    elif digest(image) != SHA256:
        raise SystemExit("Existing AppImage checksum mismatch; file left untouched")
    extract_verified(image, root)
    print("Verified official Obsidian", VERSION, "at", root)
    subprocess.run(["python3", str(Path(__file__).with_name("install.py"))], check=True)


if __name__ == "__main__":
    main()
