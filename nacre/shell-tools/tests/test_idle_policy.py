import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "idle_policy", Path(__file__).parents[1] / "idle-policy.py"
)
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class IdlePolicyTests(unittest.TestCase):
    def test_missing_override_is_seeded_without_an_idle_timeout(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            m.prepare(home)
            text = (home / ".config/nacre/hypridle.local.conf").read_text()
            self.assertNotIn("listener {", text)
            self.assertNotIn("general {", text)
            self.assertEqual(
                (home / ".config/nacre/hypridle.local.conf").stat().st_mode & 0o777,
                0o600,
            )

    def test_migration_preserves_existing_listener_and_backup(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            p = home / ".config/nacre/hypridle.local.conf"
            p.parent.mkdir(parents=True)
            listener = (
                "listener {\n timeout = 1234\n on-timeout = loginctl lock-session\n}\n"
            )
            original = "general {\n lock_cmd = hyprlock\n}\n" + listener
            p.write_text(original)
            m.prepare(home)
            self.assertIn(listener, p.read_text())
            self.assertNotIn("general {", p.read_text())
            self.assertEqual(
                next(
                    (home / ".local/state/nacre/backups").rglob("hypridle.local.conf")
                ).read_text(),
                original,
            )
