"""Candidate snapshots share deployment ownership and preserve retired owned files."""

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "release_config_map", Path(__file__).parents[1] / "releases.py"
)
releases = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(releases)


class ReleaseConfigurationMapTests(unittest.TestCase):
    def test_candidate_configuration_owner_defines_its_snapshot_map(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            repo, home = base / "repo", base / "home"
            (repo / "tools").mkdir(parents=True)
            (repo / "tools/configure.py").write_text(
                "from pathlib import Path\ndef files(root):\n"
                ' return {Path(".config/candidate-only.json"):root/"candidate.json"}\n'
            )
            with patch.object(releases, "HOME", home):
                self.assertIn(
                    home / ".config/candidate-only.json", releases.paths(repo)
                )

    def test_snapshot_omits_unowned_generated_candidate_bytecode(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            repo, home = base / "repo", base / "home"
            source = repo / "hypr/scripts/helper.py"
            source.parent.mkdir(parents=True)
            source.write_text("source")
            generated = source.parent / "__pycache__/helper.cpython-314.pyc"
            generated.parent.mkdir()
            generated.write_bytes(b"machine bytecode")
            with patch.object(releases, "HOME", home):
                paths = releases.paths(repo)
            self.assertIn(home / ".config/hypr/scripts/helper.py", paths)
            self.assertNotIn(
                home / ".config/hypr/scripts/__pycache__/helper.cpython-314.pyc", paths
            )

    def test_previous_owned_bytecode_stays_in_snapshot_for_retirement_rollback(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            repo, home = base / "repo", base / "home"
            repo.mkdir()
            manifest = home / ".local/state/nacre/configuration.json"
            manifest.parent.mkdir(parents=True)
            old = ".config/hypr/scripts/__pycache__/previous.pyc"
            manifest.write_text(json.dumps({old: "previous ownership hash"}))
            with patch.object(releases, "HOME", home):
                self.assertIn(home / old, releases.paths(repo))
