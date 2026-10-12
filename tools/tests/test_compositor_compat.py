"""Old script vocabulary delegates arguments and failures to the common facade."""

import contextlib
import importlib.util
import io
import json
from pathlib import Path
import unittest
from unittest.mock import Mock

ROOT = Path(__file__).parents[2]
SPEC = importlib.util.spec_from_file_location(
    "compositor_compat", ROOT / "hypr/scripts/compositor-compat.py"
)
compat = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(compat)


class CompositorCompatibilityTests(unittest.TestCase):
    def test_legacy_window_commands_preserve_selectors_and_defaults(self):
        examples = (
            (
                ["focus-workspace", "name:notes"],
                "switch_workspace",
                ("name:notes",),
                {"current_monitor": False},
            ),
            (
                ["focus-workspace", "2", "true"],
                "switch_workspace",
                ("2",),
                {"current_monitor": True},
            ),
            (["focus-window", "address:0x123"], "focus_window", ("address:0x123",), {}),
            (
                ["move-window", 'name:quote"\\ø', "address:0x123", "true"],
                "move_window",
                ("address:0x123",),
                {"workspace": 'name:quote"\\ø', "follow": True},
            ),
            (["move-workspace", "4", "DP-1"], "move_workspace", ("4", "DP-1"), {}),
            (["close-window", "address:0x123"], "close_window", ("address:0x123",), {}),
            (["kill-window", "address:0x123"], "kill_window", ("address:0x123",), {}),
            (
                ["fullscreen"],
                "fullscreen_window",
                ("activewindow",),
                {"mode": "fullscreen"},
            ),
            (
                ["fullscreen", "maximized", "address:0x123"],
                "fullscreen_window",
                ("address:0x123",),
                {"mode": "maximized"},
            ),
            (
                ["resize-window", "640", "480"],
                "resize_window",
                ("activewindow", 640, 480),
                {"relative": False},
            ),
            (["dpms", "enable"], "dpms", ("enable",), {"monitor": None}),
            (["dpms", "toggle", "DP-1"], "dpms", ("toggle",), {"monitor": "DP-1"}),
            (["toggle-all-float"], "toggle_workspace_floating", (), {}),
            (["exit"], "logout", (), {"graceful": True}),
        )
        for args, name, positional, keywords in examples:
            with self.subTest(command=args):
                service = Mock()
                compat.main(args, service)
                getattr(service, name).assert_called_once_with(*positional, **keywords)
                self.assertEqual(len(service.mock_calls), 1)

    def test_monitor_modes_and_mirroring_are_preserved(self):
        cases = (
            (["monitor", "DP-1", "disable"], {"output": "DP-1", "disabled": True}),
            (
                ["monitor", "DP-1", "preferred"],
                {
                    "output": "DP-1",
                    "disabled": False,
                    "mode": "preferred",
                    "position": "auto",
                    "scale": 1,
                },
            ),
            (
                ["monitor", "DP-1", "1920x1080@60", "0x0", "1.5", "eDP-1"],
                {
                    "output": "DP-1",
                    "disabled": False,
                    "mode": "1920x1080@60",
                    "position": "0x0",
                    "scale": 1.5,
                    "mirror": "eDP-1",
                },
            ),
        )
        for args, record in cases:
            with self.subTest(command=args):
                service = Mock()
                compat.main(args, service)
                service.configure_monitors.assert_called_once_with([record])

    def test_query_returns_compatible_json_and_bounded_timeout(self):
        service = Mock()
        service.query.return_value = [{"address": "0x123", "id": "window-id"}]
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            compat.main(["query", "windows"], service)
        self.assertEqual(json.loads(output.getvalue()), service.query.return_value)
        service.query.assert_called_once_with("windows", timeout=5)

    def test_performance_options_and_reset_keep_existing_values(self):
        service = Mock()
        compat.main(["performance", "true"], service)
        service.set_options.assert_called_once_with(
            {
                "animations": {"enabled": False},
                "decoration": {
                    "shadow": {"enabled": False},
                    "blur": {"enabled": False},
                    "active_opacity": 1,
                    "inactive_opacity": 1,
                    "fullscreen_opacity": 1,
                    "rounding": 0,
                },
                "general": {"gaps_in": 0, "gaps_out": 0, "border_size": 1},
            }
        )
        service.reset_mock()
        compat.main(["performance", "false"], service)
        service.reload.assert_called_once_with()
        service.set_options.assert_not_called()
        service.reset_mock()
        compat.main(["animations", "true"], service)
        service.set_options.assert_called_once_with({"animations": {"enabled": True}})

    def test_bad_boolean_unknown_command_and_service_errors_are_not_executed(self):
        for args in (
            ["performance", "true; arbitrary"],
            ["focus-workspace", "1", "yes"],
            ["not-a-command"],
        ):
            with self.subTest(command=args):
                service = Mock()
                with self.assertRaises(ValueError):
                    compat.main(args, service)
                self.assertEqual(service.mock_calls, [])
        service = Mock()
        service.close_window.side_effect = RuntimeError("Compositor unavailable")
        with self.assertRaisesRegex(RuntimeError, "Compositor unavailable"):
            compat.main(["close-window", "address:0x123"], service)

    def test_legacy_entrypoint_is_only_a_compatibility_wrapper(self):
        wrapper = (ROOT / "hypr/scripts/hyprctl-lua.sh").read_text()
        self.assertIn('compositor-compat.py" "$@"', wrapper)
        self.assertNotIn("eval", wrapper)
        for filename in ("window-close.sh", "window-trash.sh", "matrix-rest.sh"):
            source = (ROOT / "hypr/scripts" / filename).read_text()
            self.assertNotIn("hl.dispatch", source)
            self.assertNotIn("hyprctl ", source)
            self.assertIn("compositor query", source)
