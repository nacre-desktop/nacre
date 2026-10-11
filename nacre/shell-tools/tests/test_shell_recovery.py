import importlib.util
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class ShellRecoveryTests(unittest.TestCase):
    def test_bad_qml_is_rejected_before_live_source_is_changed(self):
        module = load("install")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            candidate = root / "candidate"
            live = root / "live"
            candidate.mkdir()
            live.mkdir()
            (candidate / "shell.qml").write_text(
                "Item { broken: Rectangle { }; invalid }"
            )
            (live / "marker").write_text("unchanged")
            with (
                patch.object(module, "SHELL", candidate),
                patch.object(module, "DEST", live),
            ):
                with self.assertRaises(RuntimeError):
                    module.deploy(True)
            self.assertEqual((live / "marker").read_text(), "unchanged")

    def test_runtime_fallback_preserves_rejected_code_and_preferences(self):
        module = load("shell-supervisor")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            good = root / "source.good"
            source = root / "source"
            good.mkdir()
            source.mkdir()
            (good / "shell.qml").write_text("validated")
            (source / "shell.qml").write_text("invalid")
            preferences = root / "desktop.json"
            preferences.write_text("user settings")
            with patch.object(module, "ROOT", root), patch.object(module, "GOOD", good):
                self.assertTrue(module.restore())
            self.assertEqual((source / "shell.qml").read_text(), "validated")
            self.assertEqual(preferences.read_text(), "user settings")
            self.assertEqual(
                next(root.glob("source.failed-*")).joinpath("shell.qml").read_text(),
                "invalid",
            )
