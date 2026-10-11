"""File-manager migration preserves user state while restoring archive browsing."""

import importlib.util
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location(
    "dolphin_files", ROOT / "dolphin-files.py"
)
dolphin = importlib.util.module_from_spec(spec)
spec.loader.exec_module(dolphin)


class DolphinFilesTests(unittest.TestCase):
    def test_one_time_setup_restores_zip_browsing_and_preserves_later_edits(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            config = home / ".config"
            config.mkdir()
            source = config / "dolphinrc"
            source.write_text(
                "[General]\nBrowseThroughArchives=false\nHomeUrl=/personal/files\n[Unknown]\nKeep=yes\n"
            )
            mime = config / "mimeapps.list"
            mime.write_text(
                "[Default Applications]\ntext/html=my-browser.desktop\ninode/directory=nacre-thunar.desktop\n"
            )
            dolphin.setup(home)
            prefs = dolphin.config(source)
            self.assertEqual(prefs["General"]["BrowseThroughArchives"], "true")
            self.assertEqual(prefs["General"]["HomeUrl"], "/personal/files")
            self.assertEqual(prefs["Unknown"]["Keep"], "yes")
            self.assertEqual(prefs["UiSettings"]["ColorScheme"], "Nacre")
            associations = dolphin.config(mime)["Default Applications"]
            self.assertEqual(associations["application/zip"], "org.kde.dolphin.desktop")
            self.assertEqual(associations["text/html"], "my-browser.desktop")
            self.assertEqual(
                len(
                    list(
                        (home / ".local/state/nacre/backups").glob(
                            "dolphin-*/dolphinrc"
                        )
                    )
                ),
                1,
            )
            source.write_text("[General]\nBrowseThroughArchives=false\n")
            dolphin.setup(home)
            self.assertEqual(
                dolphin.config(source)["General"]["BrowseThroughArchives"], "false"
            )

    def test_environment_uses_native_qt_and_only_unique_style_plugin(self):
        from unittest.mock import patch

        with patch.dict(
            dolphin.os.environ,
            {
                "LD_LIBRARY_PATH": "/old/runtime",
                "QML_IMPORT_PATH": "/old/qml",
                "GTK_MODULES": "thunar-module",
            },
        ):
            env = dolphin.environment()
        self.assertNotIn("LD_LIBRARY_PATH", env)
        self.assertNotIn("QML_IMPORT_PATH", env)
        self.assertNotIn("GTK_MODULES", env)
        self.assertEqual(env["QT_STYLE_OVERRIDE"], "NacreDolphin")
        self.assertEqual(env["XDG_MENU_PREFIX"], "arch-")
        self.assertTrue(env["QT_PLUGIN_PATH"].endswith("/nacre/dolphin-style"))
