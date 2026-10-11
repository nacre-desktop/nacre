"""Checked-in preset colors must remain reproducible with the pinned engine."""

import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class GeneratedPresetTests(unittest.TestCase):
    @unittest.skipUnless(
        importlib.util.find_spec("PIL") and importlib.util.find_spec("nacre_shell"),
        "Pinned palette engine unavailable",
    )
    def test_boot_seed_uses_the_current_engine_policy(self):
        from nacre_shell import ENGINE_ID
        from nacre_shell.engine import default_palette

        seed = json.loads((ROOT / "reference-style.json").read_text())
        expected = default_palette(seed["mode"], seed["variant"], seed["flavour"])
        self.assertEqual(seed["colours"], expected["colours"])
        self.assertEqual(seed["engine"], ENGINE_ID)

    @unittest.skipUnless(
        importlib.util.find_spec("PIL") and importlib.util.find_spec("nacre_shell"),
        "Pinned palette engine unavailable",
    )
    def test_generator_refreshes_both_artifacts_without_home_writes(self):
        from nacre_shell.engine import default_palette

        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            home = target / "private-home"
            home.mkdir()
            script = target / "generate-palettes.py"
            script.write_bytes(
                (ROOT.parents[1] / "tools/generate-palettes.py").read_bytes()
            )
            result = subprocess.run(
                [sys.executable, str(script), "--output-dir", str(target)],
                env={**os.environ, "HOME": str(home)},
                capture_output=True,
                text=True,
                timeout=15,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(
                json.loads((target / "reference-style.json").read_text()),
                default_palette(),
            )
            self.assertEqual(
                json.loads((target / "palette-presets.json").read_text()),
                json.loads((ROOT / "palette-presets.json").read_text()),
            )
            self.assertEqual(list(home.iterdir()), [])

    @unittest.skipUnless(
        importlib.util.find_spec("PIL") and importlib.util.find_spec("nacre_shell"),
        "Pinned palette engine unavailable",
    )
    def test_regenerated_presets_match_the_committed_artifact(self):
        from nacre_shell import ENGINE_ID

        self.assertTrue(ENGINE_ID.startswith("orient-"))
        spec = importlib.util.spec_from_file_location(
            "palette_generator", ROOT.parents[1] / "tools/generate-palettes.py"
        )
        generator = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(generator)
        self.assertEqual(
            generator.build(), json.loads((ROOT / "palette-presets.json").read_text())
        )
