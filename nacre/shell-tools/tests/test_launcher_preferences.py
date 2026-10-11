import importlib.util
import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "launcher_preferences",
    Path(__file__).resolve().parents[1] / "launcher-preferences.py",
)
prefs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prefs)


class LauncherPreferencesTests(unittest.TestCase):
    def test_favorites_and_hidden_updates_preserve_other_choices(self):
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(prefs, "PATH", Path(folder) / "launcher.json"),
        ):
            prefs.change("favorite", {"id": "one.desktop", "enabled": True})
            prefs.change("hide", {"id": "two.desktop", "enabled": True})
            prefs.change("favorite", {"id": "one.desktop", "enabled": True})
            self.assertEqual(
                prefs.load(),
                {
                    "favorites": ["one.desktop"],
                    "hidden": ["two.desktop"],
                    "recent": [],
                    "usage": {},
                },
            )
            self.assertEqual(os.stat(prefs.PATH).st_mode & 0o777, 0o600)
            prefs.change("hide", {"id": "two.desktop", "enabled": False})
            self.assertEqual(prefs.load()["hidden"], [])

    def test_action_favorites_roundtrip_alongside_apps(self):
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(prefs, "PATH", Path(folder) / "launcher.json"),
        ):
            values = [
                "music.desktop",
                "nacre-action:toggle-setting:blur",
                "nacre-action:toggle-setting:animations",
            ]
            for value in values:
                prefs.change("favorite", {"id": value, "enabled": True})
            self.assertEqual(prefs.load()["favorites"], values)
            prefs.change("favorite", {"id": values[1], "enabled": False})
            self.assertEqual(prefs.load()["favorites"], [values[0], values[2]])

    def test_invalid_preference_is_rejected_without_writes(self):
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(prefs, "PATH", Path(folder) / "launcher.json"),
        ):
            with self.assertRaises(ValueError):
                prefs.change("favorite", {"id": "bad\nname", "enabled": True})
            self.assertFalse(prefs.PATH.exists())

    def test_malformed_existing_preferences_are_preserved(self):
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(prefs, "PATH", Path(folder) / "launcher.json"),
        ):
            original = '{"favorites":42,"hidden":[]}'
            prefs.PATH.write_text(original)
            with self.assertRaises(ValueError):
                prefs.change("favorite", {"id": "one", "enabled": True})
            self.assertEqual(prefs.PATH.read_text(), original)

    def test_recent_ids_are_bounded_without_reordering_favorites(self):
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(prefs, "PATH", Path(folder) / "launcher.json"),
        ):
            prefs.change("favorite", {"id": "kept.desktop", "enabled": True})
            for index in range(15):
                prefs.change(
                    "record",
                    {"id": "nacre-menu:actions.item-" + str(index), "enabled": True},
                )
            prefs.change(
                "record", {"id": "nacre-menu:actions.item-10", "enabled": True}
            )
            data = prefs.load()
            self.assertEqual(data["favorites"], ["kept.desktop"])
            self.assertEqual(len(data["recent"]), 12)
            self.assertEqual(data["recent"][0], "nacre-menu:actions.item-10")
            self.assertEqual(len(set(data["recent"])), 12)
            self.assertEqual(data["usage"]["nacre-menu:actions.item-10"]["count"], 2)
            self.assertGreater(data["usage"]["nacre-menu:actions.item-10"]["last"], 0)
