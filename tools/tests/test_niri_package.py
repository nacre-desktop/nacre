"""The fork owns its recipe; Nacre supplies only an immutable revision."""

import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "fork_recipe", ROOT / "packaging/arch/niri/recipe.py"
)
recipe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(recipe)


class NiriPackageTests(unittest.TestCase):
    def test_stage_preserves_fork_recipe_and_pins_all_companion_files(self):
        files = [b"# fork recipe\nbuild() { upstream; }\n", b"wrapper", b"desktop"]
        with tempfile.TemporaryDirectory() as folder:
            output = Path(folder) / "PKGBUILD"
            with patch.object(recipe.subprocess, "check_output", side_effect=files) as read:
                recipe.stage("/fork", output)
            self.assertEqual(output.read_bytes(), f"_commit={recipe.SOURCE}\n".encode() + files[0])
            for index, name in enumerate(recipe.FILES):
                self.assertEqual(
                    read.call_args_list[index].args[0],
                    ["git", "-C", "/fork", "show", f"{recipe.SOURCE}:packaging/arch/{name}"],
                )
                if index:
                    self.assertEqual(output.with_name(name).read_bytes(), files[index])
            self.assertFalse((ROOT / "packaging/arch/niri/PKGBUILD").exists())
            self.assertFalse((ROOT / "packaging/arch/niri/stage.py").exists())
