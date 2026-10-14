"""Frozen panel state protects original JSON bytes and failed-write preservation."""

import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import freeze_pearl_state as reference


class PearlStateReferenceTests(unittest.TestCase):
    def test_changed_source_rejected_before_capture(self):
        with tempfile.TemporaryDirectory() as folder:
            with (
                patch.object(reference, "SOURCE", Path(folder) / "absent.py"),
                patch.object(reference.subprocess, "run") as run,
            ):
                with self.assertRaisesRegex(ValueError, "absent or changed"):
                    reference.capture(Path(folder) / "new.json")
                run.assert_not_called()

    def test_existing_reference_not_overwritten(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "frozen.json"
            path.write_bytes(b"frozen\n")
            with (
                patch.object(reference, "verify_source"),
                patch.object(reference.subprocess, "run") as run,
            ):
                with self.assertRaisesRegex(ValueError, "Refusing to overwrite"):
                    reference.capture(path)
                run.assert_not_called()
            self.assertEqual(path.read_bytes(), b"frozen\n")

    def test_corpus_records_modes_one_line_input_and_corrupt_data(self):
        data = json.loads(reference.DESTINATION.read_text())
        cases = {case["request"]["name"]: case for case in data["cases"]}
        self.assertEqual(len(cases), 11)
        target = ".local/state/nacre/shell/pearls.json"
        self.assertEqual(cases["empty-layout"]["files"][target]["mode"], "0o600")
        self.assertEqual(cases["empty-layout"]["parent_mode"], "0o700")
        self.assertEqual(
            cases["one-line-only"]["files"], cases["empty-layout"]["files"]
        )
        self.assertEqual(
            bytes.fromhex(cases["corrupt"]["files"][target]["hex"]), b"bad saved data"
        )
        self.assertEqual(cases["corrupt"]["exit"], 1)


if __name__ == "__main__":
    unittest.main()
