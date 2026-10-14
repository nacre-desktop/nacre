"""Declared desktop migration comparison preserves all semantic differences."""

import copy
import unittest
from state_schema import canonical_desktop, compatible_files


class DesktopComparisonTests(unittest.TestCase):
    def test_old_edges_and_new_independent_values_compare_without_mutation(self):
        old = {"topEdge": False, "rightEdge": False, "unknown": "keep"}
        original = copy.deepcopy(old)
        new = canonical_desktop(old)
        self.assertEqual(old, original)
        self.assertFalse(new["frameTopEnabled"])
        self.assertFalse(new["topBarEnabled"])
        self.assertFalse(new["controlCenterEnabled"])
        self.assertFalse(new["frameRightEnabled"])
        self.assertEqual(new["unknown"], "keep")
        changed = {**new, "controlCenterEnabled": True}
        self.assertNotEqual(canonical_desktop(changed), new)
        self.assertNotEqual(canonical_desktop({**new, "lipHoverDelay": 400}), new)

    def test_output_conversion_is_scoped_to_desktop_helper(self):
        value = {"stdout": {"data": {"topEdge": False}}, "files": {}}
        self.assertIn(
            "topEdge", compatible_files(value, {"helper": "other"})["stdout"]["data"]
        )
        self.assertNotIn(
            "topEdge",
            compatible_files(value, {"helper": "desktop-settings"})["stdout"]["data"],
        )
