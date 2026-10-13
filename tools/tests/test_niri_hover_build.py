import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "niri_hover_build", ROOT / "tools/build-niri-overview-hover.py"
)
build = importlib.util.module_from_spec(spec)
spec.loader.exec_module(build)


class HoverBuildTests(unittest.TestCase):
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
