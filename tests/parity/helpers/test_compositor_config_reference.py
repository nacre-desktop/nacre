"""Frozen generator outputs cannot be replaced by candidate Rust results."""

import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import freeze_compositor_config as reference


class CompositorConfigReferenceTests(unittest.TestCase):
    def test_source_guard_stops_before_any_compiler_process(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "compositor_config.py"
            source.write_text("changed reference")
            with (
                patch.object(reference, "TOOLS", root),
                patch.object(reference.subprocess, "run") as execute,
            ):
                with self.assertRaisesRegex(ValueError, "absent or changed"):
                    reference.capture(root / "new.json")
                execute.assert_not_called()

    def test_existing_reference_is_preserved_before_execution(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "reference.json"
            target.write_bytes(b"immutable reference\n")
            with (
                patch.object(reference, "verify_source"),
                patch.object(reference.subprocess, "run") as execute,
            ):
                with self.assertRaisesRegex(ValueError, "Refusing to overwrite"):
                    reference.capture(target)
                execute.assert_not_called()
            self.assertEqual(target.read_bytes(), b"immutable reference\n")

    def test_reference_covers_files_profiles_keyboard_and_strict_validation(self):
        value = json.loads(reference.DESTINATION.read_text())
        self.assertEqual(value["reference_sources"], reference.REFERENCE_SOURCES)
        cases = {case["request"]["name"]: case for case in value["cases"]}
        self.assertEqual(len(cases), 62)
        for case in cases.values():
            self.assertEqual(case["stderr"], "")
            self.assertEqual(case["files"], {})
            self.assertIn(case["exit"], (0, 1))
            self.assertTrue(case["stdout"].endswith("\n"))
        for backend, count in (("hyprland", 6), ("niri", 3)):
            result = cases["render-" + backend]
            self.assertEqual(result["exit"], 0)
            files = json.loads(result["stdout"])
            self.assertEqual(len(files), count)
            for text in files.values():
                self.assertTrue(text.endswith("\n"))
        for name in (
            "unknown-root",
            "bad-version",
            "unknown-action",
            "unknown-output",
            "bad-selector",
            "bad-float",
        ):
            case = cases["invalid-" + name]
            self.assertEqual(case["exit"], 1)
            self.assertEqual(json.loads(case["stdout"])["type"], "ValueError")
        self.assertEqual(cases["profile-named-1"]["exit"], 0)
        self.assertEqual(cases["profile-1-niri"]["exit"], 1)
        self.assertEqual(json.loads(cases["profile-0-hyprland"]["stdout"]), "")


if __name__ == "__main__":
    unittest.main()
