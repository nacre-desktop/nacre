import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "desktop_commands", Path(__file__).parents[1] / "desktop-actions.py"
)
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class DesktopActionTests(unittest.TestCase):
    def test_update_and_lock_routes_do_not_import_or_launch_brain(self):
        with patch.object(m, "launch_app") as launch:
            m.action("updates")
            self.assertEqual(
                launch.call_args.args[0][-1],
                str(m.HOME / ".local/share/nacre/shell/tools/updates.sh"),
            )
            m.action("lock")
            self.assertEqual(launch.call_args.args[0], ["loginctl", "lock-session"])

    def test_window_and_volume_commands_remain_bounded(self):
        with patch.object(m, "run") as run:
            with self.assertRaises(ValueError):
                m.action("focus", '"; injected')
            with self.assertRaises(ValueError):
                m.action("workspace", "99")
            run.assert_not_called()
            m.action("volume", "500")
            self.assertIn("100%", run.call_args.args[0])

    def test_unknown_command_cannot_become_a_shell_command(self):
        with self.assertRaises(ValueError):
            m.action("shell", "anything")
