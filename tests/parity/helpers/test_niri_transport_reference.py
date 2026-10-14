"""Transport references preserve exact request bytes and malformed-frame errors."""

import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import freeze_niri_transport as reference


class NiriTransportReferenceTests(unittest.TestCase):
    def test_source_guard_prevents_candidate_reference_capture(self):
        with tempfile.TemporaryDirectory() as directory:
            with (
                patch.object(
                    reference.reference, "SOURCE", Path(directory) / "absent.py"
                ),
                patch.object(reference.subprocess, "run") as run,
            ):
                with self.assertRaisesRegex(ValueError, "absent or changed"):
                    reference.capture(Path(directory) / "new.json")
                run.assert_not_called()

    def test_capture_never_overwrites_frozen_transport(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "reference.json"
            target.write_bytes(b"frozen\n")
            with (
                patch.object(reference.reference, "verify_source"),
                patch.object(reference.subprocess, "run") as run,
            ):
                with self.assertRaisesRegex(ValueError, "Refusing to overwrite"):
                    reference.capture(target)
                run.assert_not_called()
            self.assertEqual(target.read_bytes(), b"frozen\n")

    def test_corpus_has_exact_request_bytes_and_frame_failure_results(self):
        data = json.loads(reference.DESTINATION.read_text())
        self.assertEqual(data["source_sha256"], reference.reference.SOURCE_SHA)
        cases = {case["request"]["name"]: case for case in data["cases"]}
        self.assertEqual(len(cases), 12)
        for case in cases.values():
            observed = case["observed"]
            wire = bytes.fromhex(observed["wire_hex"])
            self.assertTrue(wire.endswith(b"\n"))
            self.assertEqual(json.loads(wire), case["request"]["request"])
            self.assertTrue(observed["connection_closed"])
            self.assertEqual(case["files"], {})
            self.assertEqual(case["stderr"], "")
        for name in ("empty", "truncated", "oversized"):
            value = json.loads(cases[name]["observed"]["stdout"])
            self.assertEqual(
                value,
                {"error": "Truncated or oversized niri reply", "type": "ValueError"},
            )
        self.assertEqual(
            json.loads(cases["u64"]["observed"]["stdout"])["FocusedWindow"]["id"],
            2**64 - 1,
        )


if __name__ == "__main__":
    unittest.main()
