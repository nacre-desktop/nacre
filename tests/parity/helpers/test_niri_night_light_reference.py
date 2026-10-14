"""Night-light capture uses synthetic records and starts no controller."""

import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import freeze_niri_night_light as reference


class NiriNightLightReferenceTests(unittest.TestCase):
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

    def test_owned_controller_and_probe_policy_are_frozen(self):
        data = json.loads(reference.DESTINATION.read_text())
        cases = {case["request"]["name"]: case for case in data["cases"]}
        self.assertEqual(len(cases), 18)
        for case in cases.values():
            self.assertEqual(case["files"], {})
            self.assertEqual(case["stderr"], "")
        self.assertEqual(
            json.loads(cases["owned-controller"]["observed"]["stdout"]), False
        )
        self.assertEqual(
            json.loads(cases["external-controller"]["observed"]["stdout"]), True
        )
        self.assertEqual(
            json.loads(cases["other-display"]["observed"]["stdout"]), False
        )
        self.assertEqual(cases["no-tools"]["observed"]["calls"], [])
        command = json.loads(cases["night_light_command-True"]["observed"]["stdout"])
        self.assertIn("--property=PartOf=graphical-session.target", command)
        self.assertEqual(
            command[-11:],
            [
                "wlsunset",
                "-t",
                "4500",
                "-T",
                "4501",
                "-S",
                "00:00",
                "-s",
                "23:59",
                "-d",
                "0",
            ],
        )


if __name__ == "__main__":
    unittest.main()
