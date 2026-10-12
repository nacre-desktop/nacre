"""The Python facade preserves transport budgets and isolates native encodings."""

import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import unittest
from unittest.mock import patch

import compositor


class CompositorBackendTests(unittest.TestCase):
    def test_import_does_not_query_or_modify_a_session(self):
        with patch.object(subprocess, "run") as run:
            spec = importlib.util.spec_from_file_location(
                "isolated_compositor", Path(compositor.__file__)
            )
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            self.assertEqual(module.backend_name, "hyprland")
            run.assert_not_called()

    def test_query_preserves_compatibility_and_adds_opaque_identity_geometry(self):
        row = {
            "address": "0xabc",
            "class": "Example",
            "title": "Title",
            "workspace": {"id": 7},
            "monitor": 2,
            "at": [3, 4],
            "size": [800, 600],
            "futureField": "retained",
        }
        with patch.object(
            compositor.backend,
            "query_result",
            return_value=subprocess.CompletedProcess([], 0, json.dumps([row])),
        ) as query:
            window = compositor.query("windows", timeout=3)[0]
            self.assertEqual(window["windowId"], "0xabc")
            self.assertEqual(window["appId"], "Example")
            self.assertEqual(window["workspaceId"], 7)
            self.assertEqual(window["geometry"], dict(x=3, y=4, width=800, height=600))
            self.assertEqual(window["futureField"], "retained")
        query.assert_called_once_with("windows", timeout=3)

    def test_query_failures_fallback_only_when_requested_and_bad_names_fail(self):
        error = subprocess.TimeoutExpired("compositor", 1)
        with patch.object(compositor.backend, "query_result", side_effect=error):
            with self.assertRaises(subprocess.TimeoutExpired):
                compositor.query("windows")
            self.assertEqual(compositor.query("windows", default=[]), [])
            with self.assertRaises(ValueError):
                compositor.query("misspelled", default=[])
        for text in ("{broken", "{}", '["not a record"]'):
            with patch.object(
                compositor.backend,
                "query_result",
                return_value=subprocess.CompletedProcess([], 0, text),
            ):
                self.assertEqual(compositor.query("windows", default=[]), [])
        with patch.object(
            compositor.backend,
            "query_result",
            return_value=subprocess.CompletedProcess([], 0, "[]"),
        ):
            with self.assertRaises(ValueError):
                compositor.query("windows", max_bytes=1)

    def test_native_window_validation_and_legacy_job_tokens_stay_in_backend(self):
        self.assertTrue(compositor.valid_window_id("0xabc"))
        self.assertEqual(compositor.window_job_token("0xabc"), "abc")
        for invalid in ("0xabc;exit", 123, None, "not-id"):
            self.assertFalse(compositor.valid_window_id(invalid))
        with patch.object(compositor.backend, "_run") as run:
            with self.assertRaises(ValueError):
                compositor.focus_window('0xabc";evil()')
            with self.assertRaises(ValueError):
                compositor.switch_workspace(True)
            with self.assertRaises(ValueError):
                compositor.switch_workspace({"output": "DP-1", "index": 2})
            run.assert_not_called()

    def test_placement_keeps_float_resize_move_order_and_per_operation_timeout(self):
        with patch.object(
            compositor.backend,
            "_run",
            return_value=subprocess.CompletedProcess([], 0, "ok"),
        ) as run:
            compositor.place_window(
                "0xabc", x=10, y=20, width=500, height=400, timeout=3
            )
            calls = run.call_args_list
            self.assertEqual(len(calls), 3)
            self.assertIn("window.float", calls[0].args[0][1])
            self.assertIn("window.resize", calls[1].args[0][1])
            self.assertIn("window.move", calls[2].args[0][1])
            self.assertTrue(all(call.kwargs["timeout"] == 3 for call in calls))

    def test_event_normalization_matches_public_qml_names(self):
        for native, expected in [
            ("activewindowv2>>abc", "window-focus"),
            ("movewindowv2>>0xabc,2", "window-workspace"),
            ("windowtitlev2>>0xabc,Title", "window-title"),
            ("monitorremoved>>DP-1", "outputs-changed"),
            ("configreloaded>>", "configuration-reloaded"),
        ]:
            self.assertEqual(compositor.parse_event(native)["type"], expected)
        self.assertNotIn("windowId", compositor.parse_event("fullscreen>>1"))
        self.assertEqual(compositor.parse_event("unknown>>payload"), {"type": "other"})

    @unittest.skipUnless(shutil.which("lua"), "Lua unavailable")
    def test_workspace_and_nested_option_values_cannot_inject_lua(self):
        with patch.object(
            compositor.backend,
            "_run",
            return_value=subprocess.CompletedProcess([], 0, "ok"),
        ) as run:
            compositor.switch_workspace('workspace-ø"; error("injection")--')
            focus = run.call_args.args[0][1]
            compositor.set_options({"example": {"value": "line\n9-ø"}})
            options = run.call_args.args[0][1]
        source = (
            'hl={dispatch=function(value)end,dsp={focus=function(value)assert(value.workspace==arg[1]);return value end},config=function(value)assert(value.example.value=="line\\n9-ø")end}\n'
            + focus
            + "\n"
            + options
        )
        result = subprocess.run(
            ["lua", "-", 'workspace-ø"; error("injection")--'],
            input=source,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_keyboard_and_monitor_rendering_keep_current_fields(self):
        text = compositor.render_keyboard({"layout": "us", "options": "caps:escape"})
        self.assertIn("input.kb_layout", text)
        text = compositor.render_monitors(
            [
                {
                    "name": "DP-1",
                    "width": 2560,
                    "height": 1440,
                    "refreshRate": 120,
                    "scale": 1.5,
                    "x": 0,
                    "y": 0,
                    "transform": 1,
                    "mirrorOf": "none",
                }
            ]
        )
        self.assertIn('mode="2560x1440@120.000"', text)
        self.assertIn("transform=1", text)
        self.assertIn('mirror=""', text)

    def test_config_error_budget_and_failure_policy_are_explicit(self):
        with patch.object(
            compositor.backend, "config_errors", side_effect=OSError("offline")
        ):
            self.assertEqual(compositor.config_errors(default=""), "")
            with self.assertRaises(OSError):
                compositor.config_errors()
