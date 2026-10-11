import importlib.util
from pathlib import Path
import tempfile
import sys
import unittest

sys.path.insert(0, str(Path(__file__).parents[1]))

spec = importlib.util.spec_from_file_location(
    "window_rules", Path(__file__).parents[1] / "window_rules.py"
)
rules = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rules)


class WindowRuleTests(unittest.TestCase):
    def test_alternation_and_case_pair_detect_conflicting_routes(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "one.lua").write_text(
                'hl.window_rule({match={class="^(Code)$"},workspace="2 silent"})'
            )
            (root / "two.lua").write_text(
                'hl.window_rule({match={class="^([Cc]ode|Cursor)$"},workspace="7 silent"})'
            )
            self.assertTrue(rules.conflicts(root))

    def test_identical_class_float_conflict_and_compatible_rules(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "one.lua").write_text(
                'hl.window_rule({match={class="^(app)$"},float=true})'
            )
            (root / "two.lua").write_text(
                'hl.window_rule({match={class="^(app)$"},float=false})'
            )
            self.assertIn("float", rules.conflicts(root)[0])
            (root / "two.lua").write_text(
                'hl.window_rule({match={class="^(app)$"},float=true})'
            )
            self.assertEqual(rules.conflicts(root), [])

    def test_extra_rules_and_normalized_global_bind_conflicts(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "base.lua").write_text(
                'hl.bind("SUPER + SHIFT + A", hl.dsp.no_op())\nhl.window_rule({match={class="^(app)$"},workspace="2"})'
            )
            extra = root / "extras.txt"
            extra.write_text(
                'hl.bind("shift+super+a", hl.dsp.no_op())\nhl.window_rule({match={class="^(app)$"},workspace="7"})'
            )
            self.assertTrue(rules.conflicts(root, [extra]))
            self.assertTrue(rules.bind_conflicts(root, [extra]))
            extra.write_text(
                'hl.define_submap("resize", function() hl.bind("SUPER + SHIFT + A", hl.dsp.no_op()) end)'
            )
            self.assertEqual(rules.bind_conflicts(root, [extra]), [])

    def test_title_scoped_exception_is_not_a_class_wide_conflict(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "one.lua").write_text(
                'hl.window_rule({match={class="^(app)$"},float=false})'
            )
            (root / "two.lua").write_text(
                'hl.window_rule({match={class="^(app)$",title="^(dialog)$"},float=true})'
            )
            self.assertEqual(rules.conflicts(root), [])
