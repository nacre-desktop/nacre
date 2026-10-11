import importlib.util
import os
from pathlib import Path
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location(
    "thunar_environment", ROOT / "thunar-files.py"
)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class RuntimeEnvironmentTests(unittest.TestCase):
    def test_thunar_does_not_export_private_native_libraries_to_children(self):
        injected = {
            "PATH": "/home/test/.local/share/nacre/palette-runtime/usr/bin:/usr/bin:/home/test/bin",
            "LD_LIBRARY_PATH": "/private/lib",
            "QT_PLUGIN_PATH": "/private/plugins",
            "QML_IMPORT_PATH": "/private/qml",
            "QML2_IMPORT_PATH": "/private/qml",
            "XCURSOR_THEME": "breeze_cursors",
        }
        with patch.dict(os.environ, injected, clear=True):
            clean = module.environment()
        self.assertEqual(clean["PATH"], "/usr/bin:/home/test/bin")
        self.assertEqual(clean["XCURSOR_THEME"], "breeze_cursors")
        for key in injected.keys() - {"PATH", "XCURSOR_THEME"}:
            self.assertNotIn(key, clean)
