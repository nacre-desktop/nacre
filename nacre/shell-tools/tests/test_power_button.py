import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "power_policy", Path(__file__).parents[1] / "power-button-policy.py"
)
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class PowerPolicyTests(unittest.TestCase):
    def test_known_blanking_binding_is_backed_up_and_wake_preferences_survive(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            path = home / ".config/nacre/host.lua"
            path.parent.mkdir(parents=True)
            wake = "hl.config({misc={mouse_move_enables_dpms=true}})\n"
            path.write_text(wake + m.LEGACY)
            self.assertTrue(m.prepare(home))
            self.assertEqual(path.read_text(), wake)
            self.assertIn(
                m.LEGACY,
                next(
                    (home / ".local/state/nacre/backups").rglob("host.lua")
                ).read_text(),
            )
            self.assertFalse(m.prepare(home))

    def test_unrecognized_private_power_action_is_not_overwritten(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            path = home / ".config/nacre/host.lua"
            path.parent.mkdir(parents=True)
            content = 'hl.bind("XF86PowerOff", custom_action)'
            path.write_text(content)
            with self.assertRaisesRegex(RuntimeError, "Private power binding differs"):
                m.prepare(home)
            self.assertEqual(path.read_text(), content)
