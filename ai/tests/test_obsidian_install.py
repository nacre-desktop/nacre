import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "obsidian_install", Path(__file__).resolve().parents[1] / "install-obsidian.py"
)
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


class ExtractionTests(unittest.TestCase):
    def test_interrupted_extraction_is_not_published_and_retry_extracts_again(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            calls = []

            def extract(command, cwd, **kwargs):
                calls.append(command)
                target = Path(cwd) / "squashfs-root"
                target.mkdir()
                (target / "AppRun").write_text("launcher")
                if len(calls) == 1:
                    raise subprocess.CalledProcessError(1, command)
                (target / "obsidian").write_text("runtime")
                (target / "resources").mkdir()
                (target / "resources/obsidian.asar").write_text("application")

            with patch.object(installer.subprocess, "run", side_effect=extract):
                with self.assertRaises(subprocess.CalledProcessError):
                    installer.extract_verified(root / "Obsidian.AppImage", root)
                self.assertFalse((root / "squashfs-root").exists())
                self.assertEqual(list(root.glob(".extract-*")), [])
                installer.extract_verified(root / "Obsidian.AppImage", root)
                installer.extract_verified(root / "Obsidian.AppImage", root)
            self.assertEqual(len(calls), 2)
            self.assertTrue((root / "squashfs-root/resources/obsidian.asar").is_file())

    def test_existing_unverified_extraction_is_preserved_without_claiming_success(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "squashfs-root").mkdir()
            (root / "squashfs-root/AppRun").write_text("existing launcher")
            with (
                patch.object(installer.subprocess, "run") as extract,
                self.assertRaises(SystemExit),
            ):
                installer.extract_verified(root / "Obsidian.AppImage", root)
            extract.assert_not_called()
            self.assertEqual(
                (root / "squashfs-root/AppRun").read_text(), "existing launcher"
            )


if __name__ == "__main__":
    unittest.main()
