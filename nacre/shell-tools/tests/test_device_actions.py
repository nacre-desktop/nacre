"""Validate Wi-Fi command boundaries without invoking a network manager."""

import importlib.util
import contextlib
import io
import json
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "device_action_commands", Path(__file__).parents[1] / "device-actions.py"
)
actions = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(actions)


class DeviceActionTests(unittest.TestCase):
    def test_wifi_connection_closes_after_success_and_keeps_failures_visible(self):
        with (
            patch.object(sys, "argv", ["device-actions.py", "wifi-connect", "Guest"]),
            patch.object(actions.subprocess, "run") as run,
            patch("builtins.input") as prompt,
        ):
            self.assertEqual(actions.main(), 0)
            prompt.assert_not_called()
            self.assertTrue(run.call_args.kwargs["check"])
        output = io.StringIO()
        with (
            patch.object(sys, "argv", ["device-actions.py", "wifi-connect", "Guest"]),
            patch.object(
                actions.subprocess,
                "run",
                side_effect=subprocess.CalledProcessError(1, "nmcli"),
            ),
            patch("builtins.input") as prompt,
            contextlib.redirect_stdout(output),
        ):
            self.assertEqual(actions.main(), 1)
            prompt.assert_called_once()
        self.assertIn("Could not connect", output.getvalue())

    def test_wifi_scan_waits_for_results_without_changing_connections(self):
        self.assertEqual(
            actions.command("wifi-scan"),
            [
                "nmcli",
                "--wait",
                "15",
                "--colors",
                "no",
                "--fields",
                "IN-USE",
                "device",
                "wifi",
                "list",
                "--rescan",
                "yes",
            ],
        )

    def test_wifi_scan_reports_completion_instead_of_raw_access_point_rows(self):
        output = io.StringIO()
        with (
            patch.object(actions, "command", return_value=["fixture-scan"]),
            patch.object(sys, "argv", ["device-actions.py", "wifi-scan"]),
            patch.object(
                actions.subprocess,
                "run",
                return_value=subprocess.CompletedProcess(
                    [], 0, stdout="IN-USE\n*\n \n", stderr=""
                ),
            ) as run,
            contextlib.redirect_stdout(output),
        ):
            actions.main()
        self.assertEqual(
            json.loads(output.getvalue()), {"message": "Wi-Fi scan complete."}
        )
        self.assertEqual(run.call_args.kwargs["timeout"], 20)

    def test_wifi_scan_failure_does_not_report_completion(self):
        output = io.StringIO()
        with (
            patch.object(sys, "argv", ["device-actions.py", "wifi-scan"]),
            patch.object(
                actions.subprocess,
                "run",
                return_value=subprocess.CompletedProcess(
                    [], 1, stdout="", stderr="Scan failed"
                ),
            ),
            contextlib.redirect_stdout(output),
        ):
            actions.main()
        self.assertEqual(json.loads(output.getvalue()), {"error": "Scan failed"})

    def test_wifi_ssid_uses_the_encoded_byte_limit(self):
        for ssid in ("x" * 32, "é" * 16, "😀" * 8):
            with self.subTest(encoded_bytes=len(ssid.encode("utf-8"))):
                command = actions.command("wifi-connect", ssid)
                self.assertEqual(
                    command, ["nmcli", "--ask", "device", "wifi", "connect", ssid]
                )
        for ssid in ("x" * 33, "é" * 17, "😀" * 9, "é" * 16 + "x"):
            with self.subTest(encoded_bytes=len(ssid.encode("utf-8"))):
                with self.assertRaises(ValueError):
                    actions.command("wifi-connect", ssid)

    def test_empty_and_control_character_ssids_remain_rejected(self):
        for ssid in ("", "fixture\nnetwork", "fixture\x00network"):
            with self.subTest(ssid=repr(ssid)):
                with self.assertRaises(ValueError):
                    actions.command("wifi-connect", ssid)
