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
    def test_all_observed_binding_actions_and_flags_preserved(self):
        expected = json.loads(
            (TOOLS / "tests/fixtures/keybindings-contract.json").read_text()
        )
        actual = capture(TOOLS.parent / "hypr/conf/keybinding.lua")

        def normalize(rows):
            return sorted(json.dumps(row, sort_keys=True) for row in rows)

        self.assertEqual(normalize(actual), normalize(expected))
        self.assertEqual(len(actual), 95)

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
