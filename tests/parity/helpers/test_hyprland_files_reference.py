"""Frozen Hyprland semantic behavior is captured before socket replacement."""

import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import freeze_hyprland_files as reference


class HyprlandFilesReferenceTests(unittest.TestCase):
    def test_capture_rejects_changed_source_before_any_helper_runs(self):
        with tempfile.TemporaryDirectory() as directory:
            with (
                patch.object(reference, "SOURCE", Path(directory) / "absent.py"),
                patch.object(reference.subprocess, "run") as run,
            ):
                with self.assertRaisesRegex(ValueError, "absent or changed"):
                    reference.capture(Path(directory) / "new.json")
                run.assert_not_called()

    def test_capture_cannot_overwrite_reference(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "reference.json"
            target.write_bytes(b"frozen\n")
            with (
                patch.object(reference, "verify_source"),
                patch.object(reference.subprocess, "run") as run,
            ):
                with self.assertRaisesRegex(ValueError, "Refusing to overwrite"):
                    reference.capture(target)
                run.assert_not_called()
            self.assertEqual(target.read_bytes(), b"frozen\n")

    def test_reference_inputs_and_effects_are_frozen(self):
        data = json.loads(reference.DESTINATION.read_text())
        cases = {case["request"]["name"]: case for case in data["cases"]}
        self.assertEqual(len(cases), 7)
        self.assertEqual(data["source_sha256"], reference.SOURCE_SHA)
        for case in cases.values():
            self.assertEqual(case["files"], {})
            self.assertEqual(case["stderr"], "")
        self.assertEqual(cases["missing-marker"]["observed"]["exit"], 1)
        self.assertEqual(
            json.loads(cases["binding-files"]["observed"]["stdout"]),
            [".config/hypr/conf/a.lua", ".config/nacre/shortcuts.lua"],
        )


if __name__ == "__main__":
    unittest.main()
