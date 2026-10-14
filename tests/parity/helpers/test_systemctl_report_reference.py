"""Real-tool formatting evidence stays read-only and tied to private queries."""

import json
import os
from pathlib import Path
import shlex
import tempfile
import unittest
from unittest.mock import patch

import freeze_systemctl_report as capture


class SystemctlReportReferenceTests(unittest.TestCase):
    def setUp(self):
        self.reference = json.loads(capture.DESTINATION.read_text())
        self.cases = {case["name"]: case for case in self.reference["cases"]}

    def test_property_order_follows_reply_not_filter_order(self):
        normal = self.cases["single"]["stdout"]
        self.assertEqual(normal, self.cases["reversed-option-order"]["stdout"])
        self.assertEqual(
            self.cases["reversed-reply-order"]["stdout"].splitlines(),
            list(reversed(normal.splitlines())),
        )
        self.assertTrue(normal.endswith("\n"))
        self.assertEqual(normal, self.cases["unselected-properties"]["stdout"])

    def test_multiple_unit_separator_and_partial_missing_unit_semantics(self):
        self.assertIn(
            "ExecMainStatus=0\n\nId=fixture.service", self.cases["two-units"]["stdout"]
        )
        later = self.cases["missing-after-valid"]
        self.assertEqual(later["exit"], 1)
        self.assertEqual(later["stdout"], self.cases["single"]["stdout"])
        first = self.cases["missing-before-valid"]
        self.assertEqual(first["exit"], 1)
        self.assertEqual(first["stdout"], "")
        self.assertEqual(len(first["wire"]), 1)
        not_found = self.cases["not-found-properties"]
        self.assertEqual(not_found["exit"], 0)
        self.assertIn("LoadState=not-found\n", not_found["stdout"])
        self.assertIn("Result=\n", not_found["stdout"])

    def test_every_wire_call_is_exactly_read_only_private_getall(self):
        for case in self.cases.values():
            for call in case["wire"]:
                self.assertEqual(call["destination"], "org.freedesktop.systemd1")
                self.assertTrue(
                    call["path"].startswith("/org/freedesktop/systemd1/unit/")
                )
                self.assertEqual(call["interface"], "org.freedesktop.DBus.Properties")
                self.assertEqual(call["member"], "GetAll")
                self.assertEqual(call["signature"], "s")
                self.assertEqual(call["body"], [""])

    def test_current_real_tool_replays_each_private_format_case(self):
        for args in capture.cases():
            with self.subTest(name=args[0]):
                actual = capture.capture_case(*args)
                expected = self.cases[args[0]]
                for field in ["stdout", "exit", "wire"]:
                    self.assertEqual(actual[field], expected[field])
                # Record stderr as evidence, but the sandbox's missing-procfs
                # warning is not part of the report text's portable contract.
                self.assertEqual(
                    "Synthetic unit not found" in actual["stderr"],
                    "Synthetic unit not found" in expected["stderr"],
                )

    def test_native_adapter_matches_captured_stdout_exit_and_exact_wire(self):
        override = os.environ.get("NACRE_SYSTEMD_REPORT_PARITY")
        paths = (
            [Path(override)]
            if override
            else [
                capture.run.ROOT / "target/release/examples/systemd-report-parity",
                capture.run.ROOT / "target/debug/examples/systemd-report-parity",
            ]
        )
        available = [
            path for path in paths if path.is_file() and os.access(path, os.X_OK)
        ]
        self.assertTrue(
            available,
            "Build systemd-report-parity from the exact revision or set NACRE_SYSTEMD_REPORT_PARITY",
        )
        binary = max(available, key=lambda path: path.stat().st_mtime_ns).resolve()
        for name, expected in self.cases.items():
            with (
                self.subTest(name=name),
                tempfile.TemporaryDirectory(
                    prefix="native-systemctl-wire-"
                ) as directory,
            ):
                trace = Path(directory) / "wire.json"
                case = {
                    "helper": "systemd-native-report-parity",
                    "stdout": "json",
                    "dbus": expected["replies"],
                }
                request = json.dumps(
                    {"units": expected["units"], "properties": expected["properties"]}
                )
                result = capture.run.execute(
                    case, shlex.join([str(binary), request]), trace
                )
                self.assertEqual(result["exit"], 0)
                self.assertEqual(
                    result["stdout"],
                    {"stdout": expected["stdout"], "exit": expected["exit"]},
                )
                self.assertEqual(
                    json.loads(trace.read_text())["dbus_calls"], expected["wire"]
                )
                self.assertEqual(result["files"], {})
                self.assertEqual(result["deleted"], [])
                self.assertEqual(result["effects"], [])

    def test_existing_reference_is_rejected_before_any_subprocess(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "reference.json"
            path.write_bytes(b"unchanged\n")
            with (
                patch.object(capture.subprocess, "check_output") as version,
                patch.object(capture, "capture_case") as execute,
            ):
                with self.assertRaisesRegex(ValueError, "refusing overwrite"):
                    capture.capture(path)
                version.assert_not_called()
                execute.assert_not_called()
            self.assertEqual(path.read_bytes(), b"unchanged\n")
