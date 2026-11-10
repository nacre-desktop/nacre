"""Shared helper protocols retained independently of the retired QML UI."""

import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import native_pearl_state

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("actions", ROOT / "device-actions.py")
actions = importlib.util.module_from_spec(spec)
spec.loader.exec_module(actions)


class SharedHelperProtocols(unittest.TestCase):
    def test_disconnect_is_bounded_to_a_valid_interface(self):
        self.assertEqual(
            actions.command("wifi-disconnect", "wlan0"),
            ["nmcli", "device", "disconnect", "wlan0"],
        )
        for interface in ["", "-all", "wlan0;reboot", "wlan0\n", "x" * 65]:
            with self.assertRaises(ValueError):
                actions.command("wifi-disconnect", interface)

    def test_network_status_ignores_ethernet_and_disconnected_wifi(self):
        with patch.object(
            actions.subprocess,
            "check_output",
            side_effect=[
                "enabled\n",
                "eth0:ethernet:connected\nwlan1:wifi:disconnected\nwlan0:wifi:connected\n",
            ],
        ):
            self.assertEqual(
                actions.network_status(), {"enabled": True, "interface": "wlan0"}
            )
        with patch.object(
            actions.subprocess,
            "check_output",
            side_effect=["disabled\n", "wlan0:wifi:disconnected\n"],
        ):
            self.assertEqual(
                actions.network_status(), {"enabled": False, "interface": ""}
            )

    def test_layout_writer_finishes_one_request_without_waiting_for_pipe_eof(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "state.json"
            process = subprocess.Popen(
                [*native_pearl_state.command(), "save"],
                env={**os.environ, "NACRE_PEARL_STATE": str(path)},
                stdin=subprocess.PIPE,
                text=True,
            )
            try:
                process.stdin.write(json.dumps({"version": 1, "windows": []}) + "\n")
                process.stdin.flush()
                self.assertEqual(process.wait(timeout=3), 0)
                self.assertEqual(json.loads(path.read_text())["windows"], [])
            finally:
                process.stdin.close()
                if process.poll() is None:
                    process.kill()
                process.wait()
