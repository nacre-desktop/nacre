"""Frozen display transactions retain validation and rollback failure semantics."""

import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import freeze_niri_monitor_transaction as reference


class NiriMonitorTransactionReferenceTests(unittest.TestCase):
    def test_capture_rejects_changed_source_before_any_helper_runs(self):
        with tempfile.TemporaryDirectory() as directory:
            with (
                patch.object(
                    reference.state_reference, "SOURCE", Path(directory) / "absent.py"
                ),
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
                patch.object(reference.state_reference, "verify_source"),
                patch.object(reference.subprocess, "run") as run,
            ):
                with self.assertRaisesRegex(ValueError, "Refusing to overwrite"):
                    reference.capture(target)
                run.assert_not_called()
            self.assertEqual(target.read_bytes(), b"frozen\n")

    def test_rollback_requests_and_failure_results_are_frozen(self):
        data = json.loads(reference.DESTINATION.read_text())
        self.assertEqual(data["source_sha256"], reference.state_reference.SOURCE_SHA)
        cases = {case["request"]["name"]: case for case in data["cases"]}
        self.assertEqual(len(cases), 21)
        self.assertEqual(
            sum(len(case["observed"]["requests"]) for case in cases.values()), 105
        )
        for case in cases.values():
            self.assertEqual(case["stderr"], "")
            self.assertEqual(case["files"], {})
        for name in ("invalid-first", "disconnected"):
            self.assertEqual(cases[name]["observed"]["requests"], [])
        failure = cases["failure-2-unexpected"]["observed"]
        self.assertEqual(len(failure["requests"]), 6)
        self.assertEqual(
            json.loads(failure["stdout"])["error"],
            "Output configuration was not applied",
        )
        rejected = json.loads(cases["rollback-unexpected"]["observed"]["stdout"])
        self.assertEqual(
            rejected["error"], "Output change failed and rollback was not accepted"
        )


if __name__ == "__main__":
    unittest.main()
