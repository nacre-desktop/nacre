"""The next backend port must preserve the current pure configuration corpus."""

import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import freeze_niri_config_query as reference


class NiriConfigQueryReferenceTests(unittest.TestCase):
    def test_capture_rejects_changed_source_before_execution(self):
        with tempfile.TemporaryDirectory() as folder:
            with (
                patch.object(
                    reference.state_reference, "SOURCE", Path(folder) / "absent.py"
                ),
                patch.object(reference.subprocess, "run") as run,
            ):
                with self.assertRaisesRegex(ValueError, "absent or changed"):
                    reference.capture(Path(folder) / "new.json")
                run.assert_not_called()

    def test_capture_never_overwrites_frozen_results(self):
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder) / "frozen.json"
            target.write_bytes(b"frozen\n")
            with (
                patch.object(reference.state_reference, "verify_source"),
                patch.object(reference.subprocess, "run") as run,
            ):
                with self.assertRaisesRegex(ValueError, "Refusing to overwrite"):
                    reference.capture(target)
                run.assert_not_called()
            self.assertEqual(target.read_bytes(), b"frozen\n")

    def test_rendering_validation_and_query_boundaries_are_frozen(self):
        data = json.loads(reference.DESTINATION.read_text())
        self.assertEqual(data["source_sha256"], reference.state_reference.SOURCE_SHA)
        cases = {case["request"]["name"]: case for case in data["cases"]}
        self.assertEqual(len(cases), 110)
        for case in cases.values():
            self.assertEqual(case["files"], {})
            self.assertEqual(case["stderr"], "")
            self.assertEqual(case["observed"]["requests"], [])
        self.assertEqual(
            cases["query-monitors"]["observed"]["probes"],
            [{"timeout": 3, "max_bytes": 8192}],
        )
        self.assertEqual(cases["query-cursor"]["observed"]["probes"], [])
        for transform in range(8):
            self.assertEqual(
                cases[f"monitor-transform-{transform}"]["observed"]["exit"], 0
            )
        self.assertEqual(cases["dimension-borderSize-1; off"]["observed"]["exit"], 1)
        result = json.loads(
            cases["job-token-niri-window:18446744073709551615"]["observed"]["stdout"]
        )
        self.assertEqual(result, "18446744073709551615")


if __name__ == "__main__":
    unittest.main()
