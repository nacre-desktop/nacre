"""Reference capture must remain Python-only and never rewrite old goldens."""

import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import freeze_menu_cli as capture


class MenuCliReferenceTests(unittest.TestCase):
    def test_frozen_cases_cover_every_action_default_and_optional_value(self):
        folder = capture.run.HERE / "menu-backend"
        for action in capture.ACTIONS:
            for form in ("default", "value"):
                with self.subTest(action=action, form=form):
                    path = folder / f"case-cli-{action}-{form}.json"
                    case = json.loads(path.read_text())
                    self.assertEqual(case["args"][0], action)
                    self.assertEqual(len(case["args"]), 1 if form == "default" else 2)
                    self.assertEqual(
                        case["reference_source_sha256"], capture.SOURCE_SHA256
                    )
                    self.assertTrue(case["compare_stderr"])

    def test_parse_errors_have_no_write_delete_process_or_ipc_effects(self):
        folder = capture.run.HERE / "menu-backend"
        errors = 0
        for path in folder.glob("expected-cli-*.json"):
            value = json.loads(path.read_text())
            if value["exit"] != 2:
                continue
            errors += 1
            with self.subTest(case=path.name):
                self.assertFalse(value["stdout"])
                self.assertTrue(value["stderr"])
                self.assertEqual(value["files"], {})
                self.assertEqual(value["deleted"], [])
                self.assertEqual(value["effects"], [])
                self.assertEqual(value["ipc"], [])
        self.assertEqual(errors, 11)

    def test_capture_refuses_changed_source_before_running_any_boundary(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "nacre/shell-tools/menu-backend.py"
            source.parent.mkdir(parents=True)
            source.write_text("changed reference")
            with (
                patch.object(capture.run, "ROOT", root),
                patch.object(capture.run, "execute") as execute,
            ):
                with self.assertRaisesRegex(ValueError, "absent or changed"):
                    capture.capture()
                execute.assert_not_called()

    def test_capture_refuses_any_existing_destination_before_execution(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "nacre/shell-tools/menu-backend.py"
            source.parent.mkdir(parents=True)
            source.write_text("immutable test source")
            folder = root / "fixtures/menu-backend"
            folder.mkdir(parents=True)
            reference = folder / "expected-cli-fixture.json"
            reference.write_bytes(b"original bytes\n")
            digest = hashlib.sha256(source.read_bytes()).hexdigest()
            with (
                patch.object(capture.run, "ROOT", root),
                patch.object(capture.run, "HERE", folder.parent),
                patch.object(capture, "SOURCE_SHA256", digest),
                patch.object(
                    capture, "cases", return_value=iter([("cli-fixture", {})])
                ),
                patch.object(capture.run, "execute") as execute,
            ):
                with self.assertRaisesRegex(ValueError, "refusing to overwrite"):
                    capture.capture()
                execute.assert_not_called()
            self.assertEqual(reference.read_bytes(), b"original bytes\n")


if __name__ == "__main__":
    unittest.main()
