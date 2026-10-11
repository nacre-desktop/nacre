"""Network snapshot correctness and read-only process boundaries."""

import importlib.util
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location(
    "network_state", ROOT / "network-state.py"
)
state = importlib.util.module_from_spec(spec)
spec.loader.exec_module(state)


class NetworkStateTests(unittest.TestCase):
    def test_escaped_colon_backslash_unicode_and_empty_fields(self):
        self.assertEqual(
            state.fields(r"*:Cafe\:lab\\wifi:AA\:BB"), ["*", "Cafe:lab\\wifi", "AA:BB"]
        )
        self.assertEqual(state.fields(":ø::"), ["", "ø", "", ""])
        for broken in ["dangling\\", r"bad\x"]:
            with self.assertRaises(ValueError):
                state.fields(broken)

    def test_ap_records_keep_active_signal_and_hidden_network(self):
        rows = (
            r"*:Cafe\:lab:00\:11\:22\:33\:44\:55:40:2437 MHz:wlan0"
            + "\n"
            + r"::00\:11\:22\:33\:44\:56:65:5180 MHz:wlan0"
        )
        points = state.access_points(rows)
        self.assertEqual(len(points), 2)
        self.assertEqual(points[0]["ssid"], "Cafe:lab")
        self.assertTrue(points[0]["active"])
        self.assertEqual(points[0]["strength"], 40)
        self.assertEqual(points[0]["frequency"], 2437)
        self.assertEqual(points[1]["ssid"], "")
        self.assertFalse(points[1]["active"])

    def test_invalid_and_truncated_ap_records_are_ignored(self):
        for row in [
            "bad",
            "*:wifi:not-a-mac:50:2437:wlan0",
            r"*:wifi:00\:11\:22\:33\:44\:55:101:2437:wlan0",
            r"*:wifi:00\:11\:22\:33\:44\:55:50:0:wlan0",
            r"*:wifi:00\:11\:22\:33\:44\:55:50:2437:-bad",
        ]:
            self.assertEqual(state.access_points(row), [])
        repeated = r"*:wifi:00\:11\:22\:33\:44\:55:50:2437:wlan0"
        self.assertEqual(len(state.access_points((repeated + "\n") * 3)), 1)

    def test_interface_requires_connected_wifi_and_valid_name(self):
        devices = "eth0:ethernet:connected\nwlan1:wifi:disconnected\nbroken\n-bad:wifi:connected\nwlan0:wifi:connected\n"
        self.assertEqual(state.connected_interface(devices), "wlan0")
        self.assertEqual(state.connected_interface("eth0:ethernet:connected"), "")

    def test_snapshot_commands_have_deadlines_and_never_scan_or_query_secrets(self):
        outputs = [
            "enabled\n",
            "wlan0:wifi:connected\n",
            r"*:Test:00\:11\:22\:33\:44\:55:50:2437 MHz:wlan0",
        ]
        with patch.object(
            state.subprocess,
            "run",
            side_effect=[
                subprocess.CompletedProcess([], 0, stdout=value) for value in outputs
            ],
        ) as run:
            result = state.snapshot()
        self.assertTrue(result["wifiEnabled"])
        self.assertEqual(result["wifiInterface"], "wlan0")
        self.assertEqual(len(result["networks"]), 1)
        for call in run.call_args_list:
            args = call.args[0]
            self.assertEqual(call.kwargs["timeout"], 8)
            self.assertTrue(call.kwargs["check"])
            self.assertEqual(call.kwargs["env"]["LC_ALL"], "C")
            self.assertNotIn("--show-secrets", args)
            self.assertNotIn("--ask", args)
        self.assertEqual(run.call_args_list[-1].args[0][-2:], ["--rescan", "no"])

    def test_disabled_radio_and_failures_are_reported_without_sensitive_output(self):
        with patch.object(state, "query", side_effect=["disabled", "", ""]):
            self.assertEqual(
                state.snapshot(), dict(wifiEnabled=False, wifiInterface="", networks=[])
            )
        with patch.object(state, "query", return_value="unknown"):
            with self.assertRaises(state.SnapshotError):
                state.snapshot()
        for error in [
            FileNotFoundError(),
            subprocess.TimeoutExpired("nmcli", 8),
            subprocess.CalledProcessError(1, "nmcli", stderr="sensitive output"),
        ]:
            with patch.object(state.subprocess, "run", side_effect=error):
                with self.assertRaises(state.SnapshotError) as caught:
                    state.query("radio", "wifi")
                self.assertNotIn("sensitive", str(caught.exception))

    def test_oversized_snapshot_is_rejected(self):
        with patch.object(
            state.subprocess,
            "run",
            return_value=subprocess.CompletedProcess(
                [], 0, stdout="x" * (2 * 1024 * 1024 + 1)
            ),
        ):
            with self.assertRaises(state.SnapshotError):
                state.query("radio", "wifi")
