"""Frozen Hyprland semantic behavior is captured before socket replacement."""

import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import freeze_hyprland_backend as reference


class HyprlandBackendReferenceTests(unittest.TestCase):
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

    def test_action_calls_validation_and_event_outputs_are_frozen(self):
        data = json.loads(reference.DESTINATION.read_text())
        self.assertEqual(data["source_sha256"], reference.SOURCE_SHA)
        cases = {case["request"]["name"]: case for case in data["cases"]}
        self.assertEqual(len(cases), 115)
        self.assertEqual(sum(case["observed"]["exit"] for case in cases.values()), 27)
        for case in cases.values():
            self.assertEqual(case["files"], {})
            self.assertEqual(case["stderr"], "")
        calls = cases["place"]["observed"]["calls"]
        self.assertEqual(len(calls), 3)
        self.assertIn("window.float", calls[0]["args"][1])
        self.assertIn("window.resize", calls[1]["args"][1])
        self.assertIn("window.move", calls[2]["args"][1])
        event = json.loads(cases["event-activewindowv2>>abc"]["observed"]["stdout"])
        self.assertEqual(event, {"type": "window-focus", "windowId": "0xabc"})


if __name__ == "__main__":
    unittest.main()
