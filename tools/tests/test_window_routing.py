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
from window_rules import conflicts


@unittest.skipUnless(shutil.which("lua"), "Lua unavailable")
class WindowRoutingTests(unittest.TestCase):
    def test_all_named_route_and_popup_contracts_preserved(self):
        expected = json.loads(
            (TOOLS / "tests/fixtures/window-routing-contract.json").read_text()
        )
        actual = capture(TOOLS.parent / "hypr/conf/windowrule.lua", "rules")
        self.assertEqual(actual, expected[-3:])
        self.assertEqual(len(actual), 3)
        self.assertEqual(len({rule["name"] for rule in actual}), 3)

    def test_generated_route_conflicts_are_not_hidden_by_tables(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "generated.lua").write_text(
                'local rows={{"^(Code)$",7},{"^(Other)$",1}};for _,r in ipairs(rows) do hl.window_rule({match={class=r[1]},workspace=tostring(r[2]).." silent"}) end'
            )
            extra = root / "private.txt"
            extra.write_text(
                'hl.window_rule({match={class="^([Cc]ode)$"},workspace="2 silent"})'
            )
            self.assertTrue(conflicts(root, [extra]))
            extra.write_text(
                'hl.window_rule({match={class="^(Code)$",title="^(Dialog)$"},workspace="2 silent"})'
            )
            self.assertEqual(conflicts(root, [extra]), [])

    def test_generated_float_conflicts_are_detected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "one.lua").write_text(
                'for _,c in ipairs({"^(App)$"}) do hl.window_rule({match={class=c},float=true}) end'
            )
            (root / "two.lua").write_text(
                'hl.window_rule({match={class="^(App)$"},float=false})'
            )
            self.assertIn("float", conflicts(root)[0])
