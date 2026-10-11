#!/usr/bin/env python3
"""Install a complete checksum-pinned official Codex package, without changing auth."""

import hashlib
from pathlib import Path
import platform
import tarfile
import tempfile
import urllib.request

VERSION = "0.159.2"
SHA256 = "9e2d29a713b94478b240dec2f10e11324cd05fad76dc43e7c639bdf8a1337a6b"
URL = f"https://github.com/openai/codex/releases/download/rust-v{VERSION}/codex-package-x86_64-unknown-linux-musl.tar.gz"

if platform.system() != "Linux" or platform.machine() != "x86_64":
    raise SystemExit("This pinned installer targets Linux x86_64")
root = Path.home() / ".local/share/nacre/ai/codex/packages"
root.mkdir(parents=True, exist_ok=True)
target = root / VERSION
if not target.exists():
    with tempfile.TemporaryDirectory(prefix=".install-", dir=root) as temporary:
        staging = Path(temporary)
        archive = staging / "package.tar.gz"
        urllib.request.urlretrieve(URL, archive)
        with archive.open("rb") as source:
            digest = hashlib.file_digest(source, "sha256").hexdigest()
        if digest != SHA256:
            raise SystemExit("Official Codex package checksum mismatch")
        unpacked = staging / "package"
        unpacked.mkdir()
        with tarfile.open(archive) as package:
            package.extractall(unpacked, filter="data")
        if not (unpacked / "codex-package.json").is_file():
            raise SystemExit("Official package manifest is missing")
        (unpacked / ".verified-archive-sha256").write_text(SHA256 + "\n")
        unpacked.rename(target)
elif (
    not (target / "codex-package.json").is_file()
    or not (target / ".verified-archive-sha256").is_file()
    or (target / ".verified-archive-sha256").read_text().strip() != SHA256
):
    raise SystemExit(
        "Existing package is not the verified pinned installation; left unchanged"
    )
print("Verified official Codex package installed at", target)
