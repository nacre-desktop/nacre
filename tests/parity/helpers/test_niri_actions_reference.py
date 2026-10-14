"""Frozen niri action IPC is independent of the candidate transport."""

import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import freeze_niri_actions as reference


class NiriActionReferenceTests(unittest.TestCase):
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

    def test_requests_retain_rounding_output_relative_ids_and_geometry_requery(self):
        data = json.loads(reference.DESTINATION.read_text())
        cases = {case["request"]["name"]: case for case in data["cases"]}
        self.assertEqual(len(cases), 78)
        self.assertEqual(
            sum(len(case["observed"]["requests"]) for case in cases.values()), 53
        )
        self.assertEqual(sum(case["observed"]["exit"] for case in cases.values()), 32)
        for case in cases.values():
            self.assertEqual(case["stderr"], "")
            self.assertEqual(case["files"], {})
        resize = cases["resize-1"]["observed"]["requests"]
        self.assertEqual(
            resize[0]["request"]["Action"]["SetWindowWidth"]["change"],
            {"SetFixed": 600},
        )
        self.assertEqual(
            resize[1]["request"]["Action"]["SetWindowHeight"]["change"],
            {"SetFixed": 402},
        )
        workspace = cases["workspace-0"]["observed"]["requests"][0]
        self.assertEqual(
            workspace["request"]["Action"]["FocusWorkspace"]["reference"],
            {"Id": 9007199254740993},
        )
        moved = cases["move-cross-output-requery"]["observed"]
        self.assertEqual(
            moved["requests"][0]["request"]["Action"]["MoveWindowToMonitor"]["output"],
            "DP-1",
        )
        self.assertEqual(
            moved["requests"][1]["request"]["Action"]["MoveFloatingWindow"]["x"],
            {"AdjustFixed": 92},
        )
        self.assertEqual(len(moved["probes"]), 3)


if __name__ == "__main__":
    unittest.main()
