"""File-manager migration preserves user state while restoring archive browsing."""

import importlib.util
from pathlib import Path
import tempfile
import unittest
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location(
    "dolphin_files", ROOT / "dolphin-files.py"
)
dolphin = importlib.util.module_from_spec(spec)
spec.loader.exec_module(dolphin)
installer_spec = importlib.util.spec_from_file_location(
    "install_dolphin", ROOT / "install-dolphin.py"
)
installer = importlib.util.module_from_spec(installer_spec)
installer_spec.loader.exec_module(installer)


class DolphinFilesTests(unittest.TestCase):
    def test_file_manager_route_explains_missing_dolphin_without_seeding(self):
        from contextlib import redirect_stderr
        from io import StringIO
        from unittest.mock import patch

        output = StringIO()
        with (
            patch.object(dolphin.shutil, "which", return_value=None),
            patch.object(dolphin, "setup") as setup,
            redirect_stderr(output),
        ):
            self.assertEqual(dolphin.main(), 127)
        setup.assert_not_called()
        self.assertIn("Dolphin is not installed", output.getvalue())

    def test_setup_skips_cleanly_when_dolphin_is_not_installed(self):
        from unittest.mock import patch

        with (
            patch.object(installer.shutil, "which", return_value=None),
            patch.object(installer.subprocess, "run") as run,
        ):
            installer.install()
        run.assert_not_called()

    def test_setup_skips_when_optional_style_packages_are_missing(self):
        from unittest.mock import patch

        with (
            patch.object(installer.shutil, "which", return_value="/usr/bin/dolphin"),
            patch.object(
                installer.subprocess,
                "run",
                return_value=SimpleNamespace(returncode=1),
            ) as run,
        ):
            installer.install()
        run.assert_called_once_with(
            ["pacman", "-Q", "kconfig", "kcolorscheme"],
            capture_output=True,
        )

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
        import tempfile
        from unittest.mock import patch

        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            plugin = (
                home / ".local/share/nacre/dolphin-style/styles/libnacre-dolphin.so"
            )
            plugin.parent.mkdir(parents=True)
            plugin.touch()
            with (
                patch.object(dolphin, "HOME", home),
                patch.dict(
                    dolphin.os.environ,
                    {
                        "LD_LIBRARY_PATH": "/old/runtime",
                        "QML_IMPORT_PATH": "/old/qml",
                        "GTK_MODULES": "thunar-module",
                    },
                ),
            ):
                env = dolphin.environment()
        self.assertNotIn("LD_LIBRARY_PATH", env)
        self.assertNotIn("QML_IMPORT_PATH", env)
        self.assertNotIn("GTK_MODULES", env)
        self.assertEqual(env["QT_STYLE_OVERRIDE"], "NacreDolphin")
        self.assertEqual(env["XDG_MENU_PREFIX"], "arch-")
        self.assertTrue(env["QT_PLUGIN_PATH"].endswith("/nacre/dolphin-style"))

    def test_environment_leaves_qt_style_at_its_default_without_plugin(self):
        import tempfile
        from unittest.mock import patch

        with tempfile.TemporaryDirectory() as directory:
            with patch.object(dolphin, "HOME", Path(directory)):
                env = dolphin.environment()
        self.assertNotIn("QT_STYLE_OVERRIDE", env)
        self.assertNotIn("QT_PLUGIN_PATH", env)
