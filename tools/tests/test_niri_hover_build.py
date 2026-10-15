import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "niri_hover_build", ROOT / "tools/build-niri-overview-hover.py"
)
build = importlib.util.module_from_spec(spec)
spec.loader.exec_module(build)


class HoverBuildTests(unittest.TestCase):
    def fixture_source(self, root):
        source = root / "source"
        source.mkdir()

        def git(*args):
            return subprocess.check_output(
                ["git", *args], cwd=source, text=True, stderr=subprocess.PIPE
            )

        git("init")
        git("config", "user.name", "Fixture")
        git("config", "user.email", "fixture@example.invalid")
        git("config", "commit.gpgsign", "false")
        git("config", "color.ui", "false")
        (source / "tracked.rs").write_text("original\n")
        (source / "unrelated.txt").write_text("keep original\n")
        git("add", ".")
        git("commit", "-m", "fixture base")
        revision = git("rev-parse", "HEAD").strip()
        (source / "tracked.rs").write_text("patched\n")
        patch_file = root / "expected.patch"
        patch_file.write_text(git("diff", "--binary", "--abbrev=7", "HEAD"))
        git("restore", "tracked.rs")
        git("config", "core.abbrev", "12")
        return source, revision, patch_file

    def test_prepare_accepts_exact_patch_with_different_git_abbreviation(self):
        with tempfile.TemporaryDirectory() as folder:
            source, revision, patch_file = self.fixture_source(Path(folder))
            with (
                patch.object(build, "SOURCE", revision),
                patch.object(build, "PATCH", patch_file),
            ):
                build.prepare(source)
                self.assertEqual((source / "tracked.rs").read_text(), "patched\n")
                self.assertEqual(build.prepare(source), source)

    def test_prepare_still_rejects_and_preserves_unrelated_changes(self):
        with tempfile.TemporaryDirectory() as folder:
            source, revision, patch_file = self.fixture_source(Path(folder))
            with (
                patch.object(build, "SOURCE", revision),
                patch.object(build, "PATCH", patch_file),
            ):
                build.prepare(source)
                unrelated = source / "unrelated.txt"
                unrelated.write_text("unrelated edit\n")
                with self.assertRaisesRegex(ValueError, "other changes"):
                    build.prepare(source)
                self.assertEqual(unrelated.read_text(), "unrelated edit\n")
                unrelated.write_text("keep original\n")
                extra = source / "unexpected.txt"
                extra.write_text("do not remove\n")
                with self.assertRaisesRegex(ValueError, "unrelated files"):
                    build.prepare(source)
                self.assertEqual(extra.read_text(), "do not remove\n")

    def test_previous_patch_upgrades_and_failed_candidate_restores_it(self):
        with tempfile.TemporaryDirectory() as folder:
            source, revision, patch_file = self.fixture_source(Path(folder))
            with (
                patch.object(build, "SOURCE", revision),
                patch.object(build, "PATCH", patch_file),
            ):
                build.prepare(source)
                previous = patch_file.with_name("niri-26.04-overview-hover-v1.patch")
                previous.write_bytes(patch_file.read_bytes())
                (source / "tracked.rs").write_text("upgraded\n")
                patch_file.write_bytes(
                    subprocess.check_output(
                        ["git", "diff", "--binary", "--abbrev=7", "HEAD"], cwd=source
                    )
                )
                (source / "tracked.rs").write_text("patched\n")
                build.prepare(source)
                self.assertEqual((source / "tracked.rs").read_text(), "upgraded\n")
                previous.write_bytes(patch_file.read_bytes())
                patch_file.write_text("invalid candidate patch\n")
                with self.assertRaises(subprocess.CalledProcessError):
                    build.prepare(source)
                self.assertEqual((source / "tracked.rs").read_text(), "upgraded\n")

    def test_install_requires_proof_for_exact_binary_patch_and_qml(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            source = home / "source"
            binary = source / "target/release/niri"
            binary.parent.mkdir(parents=True)
            binary.write_text("test compositor artifact")
            (source / "LICENSE").write_text("upstream license fixture")
            proof = dict.fromkeys(build.PROOF_FIELDS, True)
            proof.update(
                sourceRevision=build.SOURCE,
                binarySha256=build.digest(binary),
                patchSha256=build.digest(build.PATCH),
            )
            file = home / "proof.json"
            file.write_text(json.dumps(proof))
            installed = build.install(source, file, home)
            manifest = json.loads((installed / "manifest.json").read_text())
            self.assertTrue(manifest["enabled"])
            self.assertEqual(
                manifest["binarySha256"],
                hashlib.sha256((installed / "niri").read_bytes()).hexdigest(),
            )
            self.assertEqual(
                (installed / "manifest.json").stat().st_mode & 0o777, 0o600
            )
            original = (installed / "niri").read_bytes()
            proof["qmlCaptionVerified"] = False
            file.write_text(json.dumps(proof))
            with self.assertRaises(ValueError):
                build.install(source, file, home)
            self.assertEqual((installed / "niri").read_bytes(), original)
            proof["qmlCaptionVerified"] = True
            file.write_text(json.dumps(proof))
            binary.write_text("different artifact")
            with self.assertRaises(ValueError):
                build.install(source, file, home)
