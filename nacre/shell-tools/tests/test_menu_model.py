"""Independent catalog hierarchy, overlay recovery, route isolation and ranking."""

from pathlib import Path
import shutil
import subprocess
import unittest

ROOT = Path(__file__).parents[1]


class MenuModelTests(unittest.TestCase):
    def test_data_and_model_contract(self):
        node = (
            [shutil.which("node")]
            if shutil.which("node")
            else [shutil.which("nacre-ai-tools"), "node"]
        )
        if not node[0]:
            self.skipTest("Node unavailable")
        script = ROOT / "tests/test_menu_model.mjs"
        result = subprocess.run(
            [
                *node,
                str(script),
                str(ROOT.parent / "shell/modules/launcher/menu-model.js"),
                str(ROOT / "launcher-menu.json"),
            ],
            capture_output=True,
            text=True,
            timeout=15,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
