#!/usr/bin/env python3
"""Fetch the exact independently released native Orient archive for Nacre."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]


def fetch(destination):
    lock = json.loads((ROOT / "dependencies.lock.json").read_text())["orient"]
    version = lock["version"].replace("-dev.", "dev")
    release = "arch-" + version + "-" + str(lock.get("packageRelease", 1))
    fingerprint = (ROOT / "packaging/keys/nacre-repo.fingerprint").read_text().strip()
    if not re.fullmatch(r"[0-9A-F]{40}", fingerprint):
        raise ValueError("Invalid signing identity")
    if destination.exists() and any(destination.iterdir()):
        raise ValueError("Orient destination must be empty")
    destination.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="nacre-orient-release-") as temporary:
        folder = Path(temporary)
        gnupg = folder / "gnupg"
        gnupg.mkdir(mode=0o700)
        environment = {**os.environ, "GNUPGHOME": str(gnupg)}
        command = [
            "gh",
            "release",
            "download",
            release,
            "--repo",
            "nacre-desktop/orient",
            "--dir",
            str(folder),
        ]
        for pattern in (
            "orient-[0-9]*.pkg.tar.zst",
            "orient-[0-9]*.pkg.tar.zst.sig",
            "PKGBUILD",
            "PKGBUILD.sig",
            "release.json",
            "release.json.sig",
        ):
            command += ["--pattern", pattern]
        subprocess.run(command, check=True)
        try:
            subprocess.run(
                [
                    "gpg",
                    "--batch",
                    "--import",
                    str(ROOT / "packaging/keys/nacre-repo.asc"),
                ],
                env=environment,
                check=True,
                capture_output=True,
            )
            for path in (
                folder / "PKGBUILD",
                folder / "release.json",
                *folder.glob("*.pkg.tar.zst"),
            ):
                subprocess.run(
                    ["gpg", "--batch", "--verify", str(path) + ".sig", str(path)],
                    env=environment,
                    check=True,
                    capture_output=True,
                )
            receipt = json.loads((folder / "release.json").read_text())
            if receipt["sourceRevision"] != lock["revision"] or receipt[
                "version"
            ] != version + "-" + str(lock.get("packageRelease", 1)):
                raise ValueError(
                    "Published Orient source/version differs from the tested engine lock"
                )
            archives = list(folder.glob("*.pkg.tar.zst"))
            if len(archives) != 1:
                raise ValueError("Expected one native Orient archive")
            archive = archives[0]
            build = subprocess.check_output(
                ["bsdtar", "-xOf", str(archive), ".BUILDINFO"], text=True
            )
            digest = hashlib.sha256((folder / "PKGBUILD").read_bytes()).hexdigest()
            if "pkgbuild_sha256sum = " + digest not in build.splitlines():
                raise ValueError("Orient archive and signed recipe disagree")
            info = subprocess.check_output(
                ["bsdtar", "-xOf", str(archive), ".PKGINFO"], text=True
            ).splitlines()
            if (
                "pkgname = orient" not in info
                or "pkgver = " + receipt["version"] not in info
            ):
                raise ValueError("Orient archive metadata disagrees with its receipt")
            if (
                "_revision=" + lock["revision"]
                not in (folder / "PKGBUILD").read_text().splitlines()
            ):
                raise ValueError("Orient recipe source disagrees with engine lock")
            for path in folder.iterdir():
                if path.is_file():
                    shutil.copy2(path, destination / path.name)
        finally:
            subprocess.run(
                ["gpgconf", "--kill", "gpg-agent"], env=environment, capture_output=True
            )
    print("Verified independent Orient release " + release)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    fetch(parser.parse_args().destination)
