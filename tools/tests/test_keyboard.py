import subprocess
import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
import keyboard


class KeyboardTests(unittest.TestCase):
    def test_setup_from_niri_always_writes_lua_for_hyprland_and_repairs_exact_old_kdl(
        self,
    ):
        with (
            tempfile.TemporaryDirectory() as directory,
            patch.dict(
                os.environ, {"NACRE_COMPOSITOR": "niri", "XDG_CURRENT_DESKTOP": "niri"}
            ),
        ):
            home = Path(directory)
            values = {"layout": "no", "model": "pc105", "variant": "", "options": ""}
            keyboard.seed(home, values)
            target = home / ".config/nacre/keyboard.lua"
            self.assertIn("hl.config({", target.read_text())
            self.assertIn('["input.kb_layout"] = "no"', target.read_text())
            import nacre_native_compositor as native

            legacy = native.render_for_backend("niri", "render_keyboard", values)
            target.write_text(legacy)
            self.assertTrue(keyboard.seed(home, values))
            self.assertIn("hl.config({", target.read_text())
            target.write_text(legacy + "// user custom setting\n")
            self.assertFalse(keyboard.seed(home, values))
            self.assertEqual(target.read_text(), legacy + "// user custom setting\n")

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
