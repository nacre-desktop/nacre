"""Rest-view colors come from Nacre, independent of inactive legacy toolkit data."""

import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "matrix_rest", Path(__file__).parents[2] / "hypr/scripts/matrix-rest.py"
)
matrix = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(matrix)


class MatrixPaletteTests(unittest.TestCase):
    def test_canonical_surface_controls_background_and_legacy_colors_do_not(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            colors = home / ".config/nacre/colors"
            colors.mkdir(parents=True)
            for name, value in {
                "primary": "#cc4455",
                "secondary": "#8899aa",
                "onsurface": "#ddeeff",
                "surface": "#201025",
            }.items():
                (colors / name).write_text(value)
            legacy = home / ".config/rofi/colors.rasi"
            legacy.parent.mkdir(parents=True)
            legacy.write_text("surface: #00ff00; background: #0000ff;")
            with patch.object(matrix.Path, "home", return_value=home):
                first = matrix.load_palette()
                legacy.write_text("surface: #ff0000; background: #ffffff;")
                self.assertEqual(matrix.load_palette(), first)
                (colors / "surface").write_text("#405020")
                self.assertNotEqual(
                    matrix.load_palette()["background"], first["background"]
                )
