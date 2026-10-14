"""Frozen Hyprland semantic behavior is captured before socket replacement."""

import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import freeze_window_hide as reference


class WindowHideReferenceTests(unittest.TestCase):
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

    def test_records_rollback_and_original_workspace_are_frozen(self):
        data = json.loads(reference.DESTINATION.read_text())
        cases = {case["request"]["name"]: case for case in data["cases"]}
        self.assertEqual(len(cases), 12)
        path = ".cache/hypr-minimize/0xabc"
        self.assertEqual(cases["hide"]["observed"]["files"][path]["hex"], b"2\n".hex())
        self.assertEqual(cases["hide"]["observed"]["files"][path]["mode"], "0o600")
        for name in ("dispatch-failure", "restore-last", "stale"):
            self.assertNotIn(path, cases[name]["observed"]["files"])
        restore = cases["restore-last"]["observed"]["calls"]
        self.assertEqual(restore[0]["values"], {"workspace": "2", "follow": False})
        self.assertEqual(restore[1]["method"], "focus")
        self.assertEqual(
            cases["invalid-window"]["observed"]["error"],
            {"error": "Invalid window address", "type": "ValueError"},
        )


if __name__ == "__main__":
    unittest.main()
