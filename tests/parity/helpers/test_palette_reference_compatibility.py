"""The concurrent-main default must never mask other frozen-output changes."""

import unittest
from verify_colors_native import compatible_picker_record


class PaletteReferenceCompatibilityTests(unittest.TestCase):
    def test_only_one_false_picker_default_is_compatible(self):
        name = ".config/nacre/wallpaper-picker.json"
        frozen = {
            "text": '{"paletteMode": "dark", "palettePreset": "wallpaper"}\n',
            "mode": 384,
        }
        observed = {
            "text": '{"paletteMode": "dark", "paletteAutoMode": false, "palettePreset": "wallpaper"}\n',
            "mode": 384,
        }
        self.assertEqual(compatible_picker_record(name, observed, frozen), frozen)
        for changed in (
            {**observed, "text": observed["text"].replace("false", "true")},
            {**observed, "text": observed["text"].replace("dark", "light")},
            {**observed, "text": observed["text"].replace(": ", ":")},
            {**observed, "mode": 420},
        ):
            self.assertNotEqual(compatible_picker_record(name, changed, frozen), frozen)
        self.assertEqual(
            compatible_picker_record("other.json", observed, frozen), observed
        )
