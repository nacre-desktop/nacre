"""Metadata capture never opens compositor IPC or writes user state."""

import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import freeze_niri_metadata as reference


class NiriMetadataReferenceTests(unittest.TestCase):
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

    def test_metadata_identity_and_environment_are_frozen(self):
        data = json.loads(reference.DESTINATION.read_text())
        cases = {case["request"]["name"]: case for case in data["cases"]}
        self.assertEqual(len(cases), 11)
        for case in cases.values():
            self.assertEqual(case["files"], {})
            self.assertEqual(case["stderr"], "")
            self.assertEqual(case["observed"]["probes"], [])
            self.assertEqual(case["observed"]["requests"], [])
        self.assertEqual(
            json.loads(cases["session_id-False"]["observed"]["stdout"]), ""
        )
        environment = json.loads(
            cases["environment_updates-True"]["observed"]["stdout"]
        )
        self.assertNotIn("OTHER", environment)
        self.assertEqual(environment["NACRE_COMPOSITOR"], "niri")
        capabilities = json.loads(cases["capabilities"]["observed"]["stdout"])
        self.assertTrue(capabilities["nativeWindowScreenshot"])
        self.assertFalse(capabilities["windowForceKill"])


if __name__ == "__main__":
    unittest.main()
