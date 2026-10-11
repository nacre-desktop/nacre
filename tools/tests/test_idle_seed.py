"""Fresh compatibility seed preserves sleep locking without a default idle lock."""

import importlib.util
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).parents[2]
SPEC = importlib.util.spec_from_file_location(
    "idle_seed_configuration", ROOT / "tools/configure.py"
)
configure = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(configure)


class IdleSeedTests(unittest.TestCase):
    def test_actual_seed_applies_only_when_private_policy_is_missing(self):
        seed = (ROOT / "tools/defaults/hypridle.conf").read_bytes()
        self.assertIn(b"before_sleep_cmd = loginctl lock-session", seed)
        self.assertIn(
            b"after_sleep_cmd = ~/.config/hypr/scripts/hyprctl-lua.sh dpms enable", seed
        )
        self.assertNotIn(b"listener {", seed)
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            repo, home = base / "repo", base / "home"
            (repo / "hypr").mkdir(parents=True)
            (repo / "hypr/hypridle.conf").write_text(configure.IDLE_WRAPPER)
            (repo / "tools/defaults").mkdir(parents=True)
            (repo / "tools/defaults/hypridle.conf").write_bytes(seed)
            self.assertEqual(configure.idle_policy(home, repo, False), seed)
            private = home / ".config/nacre/hypridle.local.conf"
            private.parent.mkdir(parents=True)
            original = "listener { timeout = 1234 }\n"
            private.write_text(original)
            self.assertIsNone(configure.idle_policy(home, repo, False))
            self.assertEqual(private.read_text(), original)
