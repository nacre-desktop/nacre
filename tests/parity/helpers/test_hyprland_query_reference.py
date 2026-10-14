"""Frozen Hyprland semantic behavior is captured before socket replacement."""

import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import freeze_hyprland_query as reference


class HyprlandQueryReferenceTests(unittest.TestCase):
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

    def test_query_normalization_limits_and_records_are_frozen(self):
        data = json.loads(reference.DESTINATION.read_text())
        self.assertEqual(data["source_sha256"], reference.SOURCE_SHA)
        cases = {case["request"]["name"]: case for case in data["cases"]}
        self.assertEqual(len(cases), 25)
        row = json.loads(cases["valid-windows"]["observed"]["stdout"])[0]
        self.assertEqual(row["futureField"], "retained")
        self.assertEqual(row["windowId"], "0xabc")
        self.assertEqual(
            row["geometry"], {"x": -30, "y": 40, "width": 800, "height": 600}
        )
        self.assertEqual(cases["unknown"]["observed"]["calls"], [])
        self.assertEqual(
            json.loads(cases["size-bound"]["observed"]["stdout"])["error"],
            "Compositor response exceeded its size limit",
        )
        for case in cases.values():
            self.assertEqual(case["files"], {})
            self.assertEqual(case["stderr"], "")


if __name__ == "__main__":
    unittest.main()
