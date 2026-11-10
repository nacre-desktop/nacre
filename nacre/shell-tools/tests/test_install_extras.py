"""Keep the managed shortcuts loader check scoped to Hyprland installs."""

import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

TOOLS = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location(
    "install_extras_tested", TOOLS / "install-extras.py"
)
install_extras = importlib.util.module_from_spec(spec)
spec.loader.exec_module(install_extras)


class InstallExtrasBackendTests(unittest.TestCase):
    def run_install(self, backend):
        temporary = tempfile.TemporaryDirectory(prefix="install-extras-")
        self.addCleanup(temporary.cleanup)
        home = Path(temporary.name) / "home"
        home.mkdir()
        helper = Path(temporary.name) / "nacre-helper"
        helper.write_text("test helper\n")
        with (
            patch.multiple(install_extras, HOME=home, _binary=lambda: helper),
            patch.object(install_extras.compositor, "backend_name", backend),
            patch.object(install_extras.compositor, "session_id", return_value=None),
            patch.object(install_extras.subprocess, "run"),
        ):
            install_extras.install()
        return home

    def test_niri_does_not_require_a_hyprland_loader(self):
        home = self.run_install("niri")
        self.assertTrue((home / ".config/nacre/shortcuts.lua").is_file())
        self.assertFalse((home / ".config/hypr/hyprland.lua").exists())

    def test_hyprland_still_requires_the_managed_shortcuts_loader(self):
        with self.assertRaises(FileNotFoundError):
            self.run_install("hyprland")


if __name__ == "__main__":
    unittest.main()
