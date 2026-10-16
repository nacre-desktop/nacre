"""Build the checksum-pinned frozen-test renderer inside the Cargo target only."""

import fcntl
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import urllib.request


def prepare():
    spec = json.loads(Path(__file__).with_name("font-renderer.json").read_text())
    version = ".".join(str(part) for part in spec["version"])
    root = Path(__file__).resolve().parents[4]
    target = Path(os.environ.get("CARGO_TARGET_DIR", root / "target")).resolve()
    cache = target / "test-deps" / ("freetype-" + version)
    cache.mkdir(parents=True, exist_ok=True)
    with (cache / "build.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        prefix = cache / "installed"
        marker = prefix / "reference-source.sha256"
        library = prefix / "lib/libfreetype.so.6"
        if marker.exists():
            if marker.read_text().strip() != spec["sha256"] or not library.is_file():
                raise RuntimeError("Invalid frozen font renderer cache")
            return library.parent
        if prefix.exists():
            raise RuntimeError(
                "Incomplete frozen font renderer cache; inspect it first"
            )
        with tempfile.TemporaryDirectory(prefix="build-", dir=cache) as temporary:
            work = Path(temporary)
            with urllib.request.urlopen(spec["url"], timeout=30) as response:
                payload = response.read(16 * 1024 * 1024 + 1)
            if hashlib.sha256(payload).hexdigest() != spec["sha256"]:
                raise RuntimeError("Frozen font renderer source checksum mismatch")
            archive = work / "source.tar.xz"
            archive.write_bytes(payload)
            with tarfile.open(archive) as files:
                files.extractall(work, filter="data")
            source = work / ("freetype-" + version)
            built = work / "installed"
            environment = dict(os.environ, CFLAGS="-O2 -fPIC")
            commands = [
                [
                    "./configure",
                    "--prefix=" + str(built),
                    "--libdir=" + str(built / "lib"),
                    "--enable-shared",
                    "--disable-static",
                    "--with-harfbuzz=yes",
                    "--with-zlib=no",
                    "--with-bzip2=no",
                    "--with-png=no",
                    "--with-brotli=no",
                ],
                ["make", "-j2"],
                ["make", "install"],
            ]
            with (cache / "build.log").open("w") as output:
                for command in commands:
                    subprocess.run(
                        command,
                        cwd=source,
                        env=environment,
                        stdout=output,
                        stderr=subprocess.STDOUT,
                        check=True,
                        timeout=180,
                    )
            (built / marker.name).write_text(spec["sha256"] + "\n")
            built.rename(prefix)
        return library.parent


if __name__ == "__main__":
    print(prepare())
