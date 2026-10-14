"""The pre-port niri reducer corpus remains immutable reference evidence."""

import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import freeze_niri_state as reference


class NiriStateReferenceTests(unittest.TestCase):
    def test_source_guard_rejects_missing_or_changed_reference_before_execution(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "niri.py"
            source.write_text("changed reference")
            with (
                patch.object(reference, "SOURCE", source),
                patch.object(reference.subprocess, "run") as run,
            ):
                with self.assertRaisesRegex(ValueError, "absent or changed"):
                    reference.capture(root / "new.json")
                run.assert_not_called()

    def test_existing_reference_is_never_overwritten(self):
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

    def test_sequences_cover_identity_and_error_mutation_contracts(self):
        data = json.loads(reference.DESTINATION.read_text())
        self.assertEqual(data["source_sha256"], reference.SOURCE_SHA)
        self.assertEqual(data["fixture_sha256"], reference.FIXTURE_SHA)
        cases = {case["request"]["name"]: case for case in data["cases"]}
        self.assertEqual(len(cases), 22)
        self.assertEqual(sum(len(case["observations"]) for case in cases.values()), 79)
        for case in cases.values():
            self.assertEqual(case["stderr"], "")
            self.assertEqual(case["files"], {})
        maximum = cases["u64-max-identities"]["observations"][-1]["snapshot"]
        self.assertEqual(
            maximum["activeWindow"]["windowId"], "niri-window:18446744073709551615"
        )
        focus = cases["focus-failure-after-mutation"]["observations"]
        self.assertIsNotNone(focus[-2]["snapshot"]["activeWindow"])
        self.assertEqual(focus[-1]["exit"], 1)
        # Python compares True equal to native ID 1 before rejecting the
        # boolean as an opaque ID. Preserve that partial mutation too.
        self.assertEqual(
            focus[-1]["snapshot"]["activeWindow"]["windowId"], "niri-window:1"
        )
        self.assertNotEqual(
            focus[-2]["snapshot"]["activeWindow"]["windowId"], "niri-window:1"
        )
        layout = cases["layouts-failure-after-first-mutation"]["observations"][-1]
        self.assertEqual(layout["exit"], 1)
        self.assertEqual(layout["snapshot"]["windows"][0]["size"], [700, 600])


if __name__ == "__main__":
    unittest.main()
