"""Session acceptance decisions and the untouched state file are immutable."""

import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import freeze_niri_session_gate as reference


class SessionGateReferenceTests(unittest.TestCase):
    def test_source_change_is_refused_before_any_process(self):
        with tempfile.TemporaryDirectory() as folder:
            with (
                patch.object(reference, "SOURCE", Path(folder) / "missing.py"),
                patch.object(reference.subprocess, "run") as run,
            ):
                with self.assertRaisesRegex(ValueError, "absent or changed"):
                    reference.capture(Path(folder) / "new.json")
                run.assert_not_called()

    def test_reference_cannot_be_overwritten(self):
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder) / "reference.json"
            target.write_text("retain\n")
            with (
                patch.object(reference, "verify_source"),
                patch.object(reference.subprocess, "run") as run,
            ):
                with self.assertRaisesRegex(ValueError, "Refusing to overwrite"):
                    reference.capture(target)
                run.assert_not_called()
            self.assertEqual(target.read_text(), "retain\n")

    def test_boolean_identity_and_separate_unlock_gate_are_frozen(self):
        cases = json.loads(reference.DESTINATION.read_text())["cases"]
        self.assertEqual(len(cases), 270)
        indexed = {case["request"]["name"]: case["observed"] for case in cases}
        self.assertTrue(indexed["context-2-record-4-shell"]["allowed"])
        self.assertFalse(indexed["context-2-record-4-idle"]["allowed"])
        self.assertTrue(indexed["context-2-record-6-idle"]["allowed"])
        self.assertFalse(indexed["context-2-record-8-shell"]["allowed"])
        self.assertTrue(
            all(
                case["observed"]["record_before"] == case["observed"]["record_after"]
                for case in cases
            )
        )
