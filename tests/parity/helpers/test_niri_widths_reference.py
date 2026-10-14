"""Width-policy references are immutable and preserve intermediate snapshots."""

import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import freeze_niri_widths as reference
import freeze_niri_widths_x11 as x11_reference
import freeze_niri_widths_cli as cli_reference


class WidthReferenceTests(unittest.TestCase):
    def test_capture_refuses_changed_source_before_running_a_helper(self):
        with tempfile.TemporaryDirectory() as directory:
            with (
                patch.object(reference, "SOURCE", Path(directory) / "missing.py"),
                patch.object(reference.subprocess, "run") as run,
            ):
                with self.assertRaisesRegex(ValueError, "absent or changed"):
                    reference.capture(Path(directory) / "new.json")
                run.assert_not_called()

    def test_capture_never_overwrites_a_frozen_reference(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "reference.json"
            target.write_text("retain\n")
            with (
                patch.object(reference, "verify_source"),
                patch.object(reference.subprocess, "run") as run,
            ):
                with self.assertRaisesRegex(ValueError, "Refusing to overwrite"):
                    reference.capture(target)
                run.assert_not_called()
            self.assertEqual(target.read_text(), "retain\n")

    def test_snapshot_history_and_width_only_restoration_are_frozen(self):
        data = json.loads(reference.DESTINATION.read_text())
        cases = {case["request"]["name"]: case for case in data["cases"]}
        self.assertEqual(len(cases), 25)
        steps = cases["restore-floating"]["observed"]["steps"]
        self.assertEqual(steps[1]["saved"], {"10": 1600})
        self.assertEqual(steps[-1]["saved"], {})
        self.assertEqual(steps[1]["hints"]["min_width"], 1)
        self.assertEqual(steps[-1]["hints"]["min_width"], 1600)
        for name in (
            "min_height",
            "max_width",
            "max_height",
            "width_inc",
            "height_inc",
            "min_aspect",
            "max_aspect",
            "base_width",
            "base_height",
            "win_gravity",
        ):
            self.assertEqual(steps[0]["hints"][name], steps[-1]["hints"][name])
        opening = cases["one-opening-correction"]["observed"]["steps"]
        self.assertEqual(opening[-1]["resizes"], [2])
        self.assertEqual(opening[-2]["resizes"], [2])

    def test_xlib_legacy_lengths_and_unflagged_setter_fields_are_frozen(self):
        data = json.loads(x11_reference.DESTINATION.read_text())
        cases = {case["request"]["name"]: case for case in data["cases"]}
        self.assertEqual(len(cases), 19)
        self.assertIsNone(cases["length-14"]["observed"]["hints"])
        self.assertEqual(cases["length-15"]["observed"]["hints"]["base_width"], 0)
        self.assertEqual(cases["length-18"]["observed"]["hints"]["base_width"], 30)
        written = cases["length-18"]["observed"]["written_property"]
        self.assertEqual(written[5:9], [1, 600, 4000, 2000])
        self.assertEqual(written[9:], [0] * 9)

    def test_cli_grammar_and_exit_codes_are_frozen(self):
        data = json.loads(cli_reference.DESTINATION.read_text())
        cases = {tuple(case["request"]["args"]): case for case in data["cases"]}
        self.assertEqual(len(cases), 22)
        self.assertEqual(cases[("--",)]["observed"]["exit"], 2)
        self.assertEqual(cases[("--check-session",)]["observed"]["exit"], 1)
        self.assertEqual(cases[("--opening-width", "5_0")]["observed"]["exit"], 0)
        self.assertEqual(cases[("--opening-width", "٥٠")]["observed"]["exit"], 0)
