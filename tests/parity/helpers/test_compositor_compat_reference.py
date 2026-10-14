"""Frozen Hyprland semantic behavior is captured before socket replacement."""

import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import freeze_compositor_compat as reference


class CompositorCompatReferenceTests(unittest.TestCase):
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

    def test_frozen_commands_preserve_defaults_and_error_exit(self):
        data = json.loads(reference.DESTINATION.read_text())
        self.assertEqual(data["source_sha256"], reference.SOURCE_SHA)
        cases = data["cases"]
        self.assertEqual(len(cases), 27)
        for case in cases:
            self.assertEqual(case["files"], {})
            self.assertEqual(case["stderr"], "")
        focus = next(
            case
            for case in cases
            if case["request"]["argv"] == ["focus-workspace", "2"]
        )
        self.assertEqual(
            focus["observed"]["calls"][0]["kwargs"], {"current_monitor": False}
        )
        unknown = next(case for case in cases if case["request"]["argv"] == ["unknown"])
        self.assertEqual(unknown["observed"]["exit"], 2)
        self.assertEqual(
            unknown["observed"]["stderr"], "Unknown desktop command: unknown\n"
        )


if __name__ == "__main__":
    unittest.main()
