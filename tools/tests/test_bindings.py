import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest

TOOLS = Path(__file__).resolve().parents[1]
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))
from bindings import capture
from window_rules import bind_conflicts


@unittest.skipUnless(shutil.which("lua"), "Lua unavailable")
class BindingContractTests(unittest.TestCase):
    def test_all_default_binding_actions_and_flags_match_contract(self):
        expected = json.loads(
            (TOOLS / "tests/fixtures/keybindings-defaults.json").read_text()
        )
        actual = capture(TOOLS.parent / "hypr/conf/keybinding.lua")

        def normalize(rows):
            return sorted(json.dumps(row, sort_keys=True) for row in rows)

        self.assertEqual(
            normalize(actual), normalize(expected["hypr/conf/keybinding.lua"])
        )
        self.assertEqual(len(actual), 93)

    def test_defaults_have_no_conflicts_or_personal_app_commands(self):
        root = TOOLS.parent
        self.assertEqual(
            bind_conflicts(
                root / "hypr/conf", [root / "nacre/shell-tools/shortcuts.lua"]
            ),
            [],
        )
        rows = capture(root / "hypr/conf/keybinding.lua")
        rows += capture(root / "nacre/shell-tools/shortcuts.lua")
        commands = [
            row["action"]["args"][0]
            for row in rows
            if row["action"]["method"] == "exec_cmd"
        ]
        for name in (
            "google-chrome-stable",
            "-- code",
            "-- cursor",
            "-- discord",
            "-- spotify",
            "matrix-rest.sh",
            "startup-apps.sh",
        ):
            self.assertFalse(any(name in command for command in commands), name)
        keys = {row["keys"]: row for row in rows if not row["submap"]}
        self.assertEqual(keys["SUPER + Q"]["flags"]["description"], "Nacre:close")
        self.assertEqual(
            keys["SUPER + Q"]["action"], {"method": "window.close", "args": {}}
        )
        self.assertNotIn("SUPER + CTRL + Q", keys)
        for action in ("put-away", "restore", "restore-all", "force-close"):
            self.assertFalse(
                any(
                    row["flags"].get("description") == "Nacre:" + action for row in rows
                )
            )
        for key, role in (
            ("SUPER + RETURN", "terminal"),
            ("SUPER + E", "files"),
            ("SUPER + SHIFT + B", "browser"),
        ):
            self.assertIn("nacre-app " + role, keys[key]["action"]["args"][0])
        self.assertEqual(
            keys["SUPER + SPACE"]["flags"]["description"], "Nacre:launcher"
        )
        self.assertEqual(keys["SUPER + A"]["flags"]["description"], "Nacre:ai-sidebar")
        self.assertEqual(keys["SUPER + L"]["action"]["args"], ["loginctl lock-session"])
        self.assertEqual(keys["SUPER + ESCAPE"]["action"], keys["SUPER + X"]["action"])
        self.assertEqual(keys["Print"]["action"], keys["SUPER + SHIFT + S"]["action"])
        for key, number in [(str(n), n) for n in range(1, 10)] + [("0", 10)]:
            self.assertEqual(
                keys["SUPER + " + key]["action"]["args"][0]["workspace"], number
            )
            self.assertEqual(
                keys["SUPER + SHIFT + " + key]["action"]["args"][0]["workspace"], number
            )
        for key in (
            "SUPER + S",
            "SUPER + CTRL + S",
            "SUPER + CTRL + E",
            "SUPER + CTRL + Page_Up",
            "SUPER + CTRL + Page_Down",
            "SUPER + F1",
        ):
            self.assertIn(key, keys)
        self.assertFalse(any("code:" in key for key in keys))

    def test_generated_rows_conflict_with_literal_private_shortcut(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "map.lua").write_text(
                'for n=1,2 do hl.bind("SUPER + "..n,hl.dsp.focus({workspace=n})) end'
            )
            extra = root / "private.txt"
            extra.write_text('hl.bind("super+1",hl.dsp.exec_cmd("never run"))')
            self.assertTrue(bind_conflicts(root, [extra]))
            extra.write_text(
                'hl.define_submap("resize",function() hl.bind("super+1",hl.dsp.exec_cmd("never run")) end)'
            )
            self.assertEqual(bind_conflicts(root, [extra]), [])

    def test_restricted_capture_cannot_execute_actions_or_read_environment(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "bad.lua"
            for expression in (
                'os.execute("echo forbidden")',
                'io.popen("echo forbidden")',
                'dofile("elsewhere")',
                'require("os")',
            ):
                source.write_text(expression)
                with self.assertRaisesRegex(RuntimeError, "Cannot capture"):
                    capture(source)
            source.write_text(
                'hl.bind("SUPER + A",function() error("must not execute") end)'
            )
            self.assertEqual(capture(source)[0]["action"]["method"], "callback")
