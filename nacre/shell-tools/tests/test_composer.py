import importlib.util
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / (name + ".py"))
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m


m = load("composer")


class ComposerTests(unittest.TestCase):
    def test_drafts_are_private_isolated_and_recover_after_reloading(self):
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(m, "ROOT", Path(folder)),
        ):
            m.save("thread-one", "Draft one", [])
            m.save("thread-two", "Draft two", [])
            self.assertEqual(m.load("thread-one")["text"], "Draft one")
            self.assertEqual(m.load("thread-two")["text"], "Draft two")
            self.assertEqual(m.key_path("thread-one").stat().st_mode & 0o777, 0o600)

    def test_selected_files_are_local_valid_and_deduplicated(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "screen.png"
            path.write_bytes(b"fixture")
            result = m.attachments([str(path), path.as_uri()])
            self.assertEqual(len(result), 1)
            self.assertTrue(result[0]["image"])
            with self.assertRaises(ValueError):
                m.attachments(["https://example.test/photo.png"])
            with self.assertRaises(ValueError):
                m.attachments([str(path.parent / "missing.txt")])

    def test_native_codex_input_includes_local_image(self):
        peer = load("sidebar-chat")
        inputs = peer.model_input(
            "Look at this", [dict(path="/fixture/image.png", image=True)]
        )
        self.assertEqual(
            inputs,
            [
                dict(type="text", text="Look at this"),
                dict(type="localImage", path="/fixture/image.png"),
            ],
        )
