"""Generated binding capture protects parser provenance and exact records."""

import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import freeze_niri_bindings as reference


class NiriBindingsReferenceTests(unittest.TestCase):
    def test_changed_source_rejected_before_execution(self):
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

    def test_existing_reference_never_overwritten(self):
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder) / "frozen.json"
            target.write_bytes(b"frozen\n")
            with (
                patch.object(reference, "verify_source"),
                patch.object(reference.subprocess, "run") as run,
            ):
                with self.assertRaisesRegex(ValueError, "Refusing to overwrite"):
                    reference.capture(target)
                run.assert_not_called()
            self.assertEqual(target.read_bytes(), b"frozen\n")

    def test_corpus_preserves_generated_keys_and_private_override_rejection(self):
        data = json.loads(reference.DESTINATION.read_text())
        self.assertEqual(data["bindings_sha256"], reference.BINDINGS_SHA)
        cases = {case["request"]["name"]: case for case in data["cases"]}
        self.assertEqual(len(cases), 7)
        records = json.loads(cases["generated-capture"]["observed"]["stdout"])
        self.assertEqual(len(records), 83)
        query = json.loads(cases["generated-query"]["observed"]["stdout"])
        self.assertTrue(
            any(row["key"] == "G" and row["modmask"] == 64 for row in query)
        )
        self.assertEqual(cases["private-override"]["observed"]["exit"], 1)
        self.assertEqual(json.loads(cases["ignored-lines"]["observed"]["stdout"]), [])
        for case in cases.values():
            self.assertEqual(case["files"], {})
            self.assertEqual(case["stderr"], "")


if __name__ == "__main__":
    unittest.main()
