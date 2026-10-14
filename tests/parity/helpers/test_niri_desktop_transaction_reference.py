"""File publication references preserve validation order and old configuration."""

import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import freeze_niri_desktop_transaction as reference


class NiriDesktopTransactionReferenceTests(unittest.TestCase):
    def test_changed_source_rejected_before_capture(self):
        with tempfile.TemporaryDirectory() as folder:
            with (
                patch.object(reference.reference, "SOURCE", Path(folder) / "absent.py"),
                patch.object(reference.subprocess, "run") as run,
            ):
                with self.assertRaisesRegex(ValueError, "absent or changed"):
                    reference.capture(Path(folder) / "new.json")
                run.assert_not_called()

    def test_existing_outputs_cannot_be_overwritten(self):
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder) / "frozen.json"
            target.write_bytes(b"frozen\n")
            with (
                patch.object(reference.reference, "verify_source"),
                patch.object(reference.subprocess, "run") as run,
            ):
                with self.assertRaisesRegex(ValueError, "Refusing to overwrite"):
                    reference.capture(target)
                run.assert_not_called()
            self.assertEqual(target.read_bytes(), b"frozen\n")

    def test_file_modes_cleanup_and_rollback_are_recorded(self):
        data = json.loads(reference.DESTINATION.read_text())
        self.assertEqual(data["source_sha256"], reference.reference.SOURCE_SHA)
        cases = {case["request"]["name"]: case["observed"] for case in data["cases"]}
        self.assertEqual(len(cases), 13)
        target = ".config/nacre/niri/desktop.kdl"
        self.assertEqual(cases["new"]["files"][target]["mode"], "0o600")
        self.assertEqual(
            [call["kind"] for call in cases["new"]["calls"]],
            ["query", "validate", "reload"],
        )
        self.assertEqual(
            [call["kind"] for call in cases["stored-monitors"]["calls"]],
            ["validate", "reload"],
        )
        for name in ("validation-failure-existing", "reload-failure-existing"):
            self.assertEqual(
                cases[name]["files"][target]["hex"],
                cases[name]["initial_files"][target]["hex"],
            )
        for name in ("validation-failure-new", "reload-failure-new"):
            self.assertNotIn(target, cases[name]["files"])
        for case in cases.values():
            self.assertFalse(
                any(Path(name).name.startswith("tmp") for name in case["files"])
            )


if __name__ == "__main__":
    unittest.main()
