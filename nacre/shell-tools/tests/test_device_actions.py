"""Validate Wi-Fi command boundaries without invoking a network manager."""

import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location(
    "device_action_commands", Path(__file__).parents[1] / "device-actions.py"
)
actions = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(actions)


class DeviceActionTests(unittest.TestCase):
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
