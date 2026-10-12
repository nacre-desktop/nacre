import importlib.util
from pathlib import Path
import unittest
import tempfile
import json
import subprocess
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

    def test_ai_launches_on_current_workspace_without_an_explicit_assignment(self):
        with tempfile.TemporaryDirectory() as directory:
            with (
                patch.object(m, "HOME", Path(directory)),
                patch.object(m.compositor, "query", return_value=[]) as query,
                patch.object(m.compositor, "switch_workspace") as switch,
                patch.object(m, "launch_app") as launch,
            ):
                for command in ("new", "resume", "tasks"):
                    m.action(command)
                self.assertEqual(launch.call_count, 3)
                query.assert_called_once_with("windows", timeout=10)
                switch.assert_not_called()

    def test_explicit_ai_workspace_is_selected_before_launch(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            profile = home / ".config/nacre/profile.json"
            profile.parent.mkdir(parents=True)
            profile.write_text(json.dumps({"workspaceCount": 10, "aiWorkspace": 9}))
            calls = []
            with (
                patch.object(m, "HOME", home),
                patch.object(
                    m.compositor,
                    "switch_workspace",
                    side_effect=lambda workspace, **kw: calls.append((workspace, kw)),
                ),
                patch.object(
                    m, "launch_app", side_effect=lambda command: calls.append(command)
                ),
            ):
                m.action("new")
            self.assertEqual(calls[0], (9, {"current_monitor": True, "timeout": 10}))
            self.assertEqual(calls[1][0], "kitty")

    def test_unknown_command_cannot_become_a_shell_command(self):
        with self.assertRaises(ValueError):
            m.action("shell", "anything")

    def test_window_query_fallback_and_dispatch_error_policy_are_preserved(self):
        with patch.object(
            m.compositor,
            "query",
            side_effect=subprocess.CalledProcessError(1, "compositor"),
        ):
            self.assertEqual(m.compositor_windows(), [])
        with patch.object(
            m.compositor, "query", side_effect=ValueError("bad response")
        ):
            with self.assertRaises(ValueError):
                m.compositor_windows()
        with patch.object(
            m.compositor,
            "focus_window",
            side_effect=subprocess.CalledProcessError(
                1, "compositor", stderr="offline"
            ),
        ):
            with self.assertRaisesRegex(RuntimeError, "offline"):
                m.action("focus", "0x123")
