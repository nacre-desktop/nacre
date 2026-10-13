import importlib.util
import socket
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
    def test_large_buffer_policy_is_scoped_to_child_and_preserves_override(self):
        module = load("shell-supervisor")
        with tempfile.TemporaryDirectory() as directory:
            runtime = Path(directory)
            with socket.socket(socket.AF_UNIX) as display:
                display.bind(str(runtime / "wayland-test"))
                for override in (None, "524288"):
                    with self.subTest(override=override):
                        source = {
                            "XDG_RUNTIME_DIR": str(runtime),
                            "WAYLAND_DISPLAY": "wayland-test",
                        }
                        if override:
                            source["MALLOC_MMAP_THRESHOLD_"] = override
                        with (
                            patch.dict(module.os.environ, source, clear=True),
                            patch.object(
                                module.compositor,
                                "environment_updates",
                                return_value={},
                            ),
                        ):
                            child = module.environment()
                            self.assertEqual(
                                child["MALLOC_MMAP_THRESHOLD_"], override or "131072"
                            )
                            self.assertEqual(dict(module.os.environ), source)

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
