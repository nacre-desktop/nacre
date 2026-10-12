import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
import keyboard


class KeyboardTests(unittest.TestCase):
    def test_system_layout_and_variant_preserved(self):
        output = (
            " X11 Layout: de\n X11 Variant: nodeadkeys\n X11 Options: caps:escape\n"
        )
        with patch.object(
            keyboard.subprocess,
            "run",
            return_value=subprocess.CompletedProcess([], 0, output),
        ):
            values = keyboard.system_keyboard()
        self.assertEqual(values["layout"], "de")
        self.assertEqual(values["variant"], "nodeadkeys")
        self.assertEqual(values["options"], "caps:escape")

    def test_unavailable_system_configuration_falls_back_to_us(self):
        with patch.object(keyboard.subprocess, "run", side_effect=FileNotFoundError):
            self.assertEqual(keyboard.system_keyboard()["layout"], "us")

    def test_existing_user_override_is_never_replaced(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            self.assertTrue(keyboard.seed(home, {"layout": "us"}))
            target = home / ".config/nacre/keyboard.lua"
            target.write_text("-- user-owned\n")
            self.assertFalse(keyboard.seed(home, {"layout": "fr"}))
            self.assertEqual(target.read_text(), "-- user-owned\n")
