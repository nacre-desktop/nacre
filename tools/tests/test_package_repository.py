"""Repository promotion validates real package metadata, signatures and retention."""

import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "arch_repository", ROOT / "packaging/arch/repository.py"
)
repository = importlib.util.module_from_spec(spec)
spec.loader.exec_module(repository)


@unittest.skipUnless(
    all(shutil.which(tool) for tool in ("gpg", "repo-add", "bsdtar", "vercmp")),
    "Arch repository tools unavailable",
)
class RepositoryTests(unittest.TestCase):
    def test_bootstrap_and_public_key_have_the_same_pinned_signing_identity(self):
        with tempfile.TemporaryDirectory() as folder:
            listing = subprocess.check_output(
                [
                    "gpg",
                    "--homedir",
                    folder,
                    "--batch",
                    "--with-colons",
                    "--show-keys",
                    str(ROOT / "packaging/keys/nacre-repo.asc"),
                ],
                text=True,
                stderr=subprocess.DEVNULL,
            )
        fingerprint = next(
            line.split(":")[9]
            for line in listing.splitlines()
            if line.startswith("fpr:")
        )
        self.assertEqual(
            fingerprint,
            (ROOT / "packaging/keys/nacre-repo.fingerprint").read_text().strip(),
        )
        self.assertIn(
            "fingerprint=" + fingerprint + "\n",
            (ROOT / "packaging/arch/bootstrap.sh").read_text(),
        )

    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="nacre-repository-proof-")
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name)
        self.root = self.base / "source"
        self.artifacts = self.base / "artifacts"
        (self.root / "packaging/keys").mkdir(parents=True)
        (self.root / "packaging/arch").mkdir()
        (self.root / "packaging/arch/bootstrap.sh").write_text("#!/bin/sh\nexit 0\n")
        (self.root / "packaging/sessions.json").write_text(
            json.dumps(
                {
                    name: {"package": "nacre-session-" + name}
                    for name in ("niri", "hyprland")
                }
            )
        )
        (self.root / "dependencies.lock.json").write_text(
            '{"orient":{"version":"0.1.0-dev.6"}}'
        )
        for name in ("nacre", "orient"):
            (self.artifacts / name).mkdir(parents=True)
        (self.artifacts / "nacre/PKGBUILD").write_text(
            "source=('nacre::git+https://github.com/nacre-desktop/nacre.git#commit="
            + "a" * 40
            + "')\n"
        )
        (self.artifacts / "nacre/.SRCINFO").write_text("pkgbase = nacre\n")
        (self.artifacts / "installed-runtime.log").write_text(
            "FRESH_USER_PACKAGE_PROOF_OK\n" * 3
        )
        self.recipe_digest = hashlib.sha256(
            (self.artifacts / "nacre/PKGBUILD").read_bytes()
        ).hexdigest()
        self.make_packages(1)

    def archive(self, name, version, dependencies=()):
        with tempfile.TemporaryDirectory(dir=self.base) as temporary:
            folder = Path(temporary)
            info = [
                f"pkgname = {name}",
                f"pkgver = {version}",
                "arch = x86_64",
                "pkgdesc = Test fixture",
                "builddate = 1",
                "packager = Fixture",
                "size = 0",
                "license = MIT",
                *("depend = " + dep for dep in dependencies),
            ]
            (folder / ".PKGINFO").write_text("\n".join(info) + "\n")
            (folder / ".BUILDINFO").write_text(
                "pkgbuild_sha256sum = " + self.recipe_digest + "\n"
            )
            path = (
                self.artifacts
                / ("orient" if name == "orient" else "nacre")
                / f"{name}-{version}-x86_64.pkg.tar.zst"
            )
            subprocess.run(
                [
                    "bsdtar",
                    "--zstd",
                    "-cf",
                    str(path),
                    "-C",
                    str(folder),
                    ".PKGINFO",
                    ".BUILDINFO",
                ],
                check=True,
            )
            return path

    def make_packages(self, sequence):
        for path in (self.artifacts / "nacre").glob("*.pkg.tar.zst"):
            path.unlink()
        version = f"0.1.0dev2.r{sequence}.gaaaaaaaaaaaa"
        self.archive("nacre", version + "-1", ("orient=0.1.0dev6",))
        for name in (
            "nacre-ai",
            "nacre-brain",
            "nacre-session-niri",
            "nacre-session-hyprland",
        ):
            self.archive(name, version + "-1", ("nacre=" + version,))
        if not list((self.artifacts / "orient").glob("*.pkg.tar.zst")):
            self.archive("orient", "0.1.0dev6-1")

    def test_missing_runtime_proof_and_wrong_recipe_are_rejected(self):
        proof = self.artifacts / "installed-runtime.log"
        proof.write_text("FRESH_USER_PACKAGE_PROOF_OK\n")
        with self.assertRaisesRegex(ValueError, "all three"):
            repository.verify_artifacts(self.root, self.artifacts)
        proof.write_text("FRESH_USER_PACKAGE_PROOF_OK\n" * 3)
        (self.artifacts / "nacre/PKGBUILD").write_text(
            (self.artifacts / "nacre/PKGBUILD").read_text() + "# changed recipe\n"
        )
        with self.assertRaisesRegex(ValueError, "tested Core recipe"):
            repository.verify_artifacts(self.root, self.artifacts)

    def test_orient_stays_separate_and_its_tested_version_is_required(self):
        paths, records, source = repository.verify_artifacts(self.root, self.artifacts)
        self.assertEqual(source, "a" * 40)
        self.assertIn("orient", paths)
        self.assertIn("orient=0.1.0dev6", records["nacre"]["dependencies"])
        (self.root / "dependencies.lock.json").write_text(
            '{"orient":{"version":"0.1.0-dev.7"}}'
        )
        with self.assertRaisesRegex(ValueError, "separately tested Orient"):
            repository.verify_artifacts(self.root, self.artifacts)

    def test_signed_repository_retains_previous_cut_and_refuses_tampering(self):
        gnupg = self.base / "gnupg"
        gnupg.mkdir(mode=0o700)
        with patch.dict(os.environ, {"GNUPGHOME": str(gnupg)}):
            try:
                subprocess.run(
                    [
                        "gpg",
                        "--batch",
                        "--pinentry-mode",
                        "loopback",
                        "--passphrase",
                        "",
                        "--quick-generate-key",
                        "Repository fixture <fixture@example.invalid>",
                        "ed25519",
                        "sign",
                        "1d",
                    ],
                    check=True,
                    capture_output=True,
                )
                listing = repository.run(
                    "gpg", "--batch", "--with-colons", "--list-secret-keys"
                )
                fingerprint = next(
                    line.split(":")[9]
                    for line in listing.splitlines()
                    if line.startswith("fpr:")
                )
                (self.root / "packaging/keys/nacre-repo.asc").write_text(
                    repository.run("gpg", "--batch", "--armor", "--export", fingerprint)
                )
                first = self.base / "first"
                record = repository.build(
                    self.root, self.artifacts, first, fingerprint, "a" * 40, "123"
                )
                previous = first / "arch/dev/x86_64"
                self.assertFalse(any(path.is_symlink() for path in first.rglob("*")))
                repository.run(
                    "gpg",
                    "--verify",
                    str(previous / "nacre-dev.db.sig"),
                    str(previous / "nacre-dev.db"),
                )
                self.make_packages(2)
                second = self.base / "second"
                repository.build(
                    self.root,
                    self.artifacts,
                    second,
                    fingerprint,
                    "a" * 40,
                    "124",
                    previous,
                )
                old_archive = previous / record["packages"]["nacre"]["filename"]
                self.assertTrue(
                    (second / "arch/dev/x86_64" / old_archive.name).is_file()
                )
                old_archive.write_bytes(old_archive.read_bytes() + b"tampered")
                with self.assertRaisesRegex(ValueError, "checksum changed"):
                    repository.build(
                        self.root,
                        self.artifacts,
                        self.base / "bad",
                        fingerprint,
                        "a" * 40,
                        "125",
                        previous,
                    )
            finally:
                subprocess.run(["gpgconf", "--kill", "gpg-agent"], capture_output=True)
