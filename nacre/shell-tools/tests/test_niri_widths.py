"""Constraint restoration, dialog safety and manual-width ownership."""

import unittest
import native_niri_widths as widths


class X11Fixture:
    def __init__(self):
        self.value = widths.SizeHints()
        self.value.flags = widths.MIN_SIZE | widths.MAX_SIZE
        self.value.min_width, self.value.min_height = 1600, 600
        self.value.max_width, self.value.max_height = 4000, 2000
        self.ordinary_window = True

    def windows(self):
        return [10]

    def identity(self, window):
        return {"fixture"}, "Window"

    def ordinary(self, window):
        return self.ordinary_window

    def hints(self, window):
        return widths.SizeHints.from_buffer_copy(bytes(self.value))

    def set_hints(self, window, value):
        self.value = widths.SizeHints.from_buffer_copy(bytes(value))


class WidthTests(unittest.TestCase):
    def setUp(self):
        self.x11 = X11Fixture()
        self.policy = widths.Policy(self.x11)
        self.addCleanup(self.policy.stop)
        self.window = {
            "id": 1,
            "app_id": "Fixture",
            "title": "Window",
            "is_floating": False,
        }
        self.policy.event({"WindowsChanged": {"windows": [self.window]}})

    def test_width_only_and_restore_after_floating(self):
        self.policy.reconcile()
        self.assertEqual(self.x11.value.min_width, 1)
        self.assertEqual(self.x11.value.min_height, 600)
        self.assertEqual(self.x11.value.max_width, 4000)
        self.assertEqual(self.x11.value.max_height, 2000)
        self.window["is_floating"] = True
        self.policy.event({"WindowOpenedOrChanged": {"window": self.window}})
        self.policy.reconcile()
        self.assertEqual(self.x11.value.min_width, 1600)

    def test_app_hint_change_updates_restoration_without_resizing(self):
        self.policy.reconcile()
        self.x11.value.min_width = 2400
        self.policy.reconcile()
        self.assertEqual(self.x11.value.min_width, 1)
        self.policy.close()
        self.assertEqual(self.x11.value.min_width, 2400)

    def test_fixed_dialog_and_ambiguous_floating_windows_are_preserved(self):
        self.x11.ordinary_window = False
        self.policy.reconcile()
        self.assertEqual(self.x11.value.min_width, 1600)
        self.x11.ordinary_window = True
        self.x11.value.max_width = 1600
        self.policy.reconcile()
        self.assertEqual(self.x11.value.min_width, 1600)
        self.x11.value.max_width = 4000
        duplicate = dict(self.window, id=2, is_floating=True)
        self.policy.event({"WindowOpenedOrChanged": {"window": duplicate}})
        self.policy.reconcile()
        self.assertEqual(self.x11.value.min_width, 1600)

    def test_app_removal_of_minimum_does_not_restore_stale_hint(self):
        self.policy.reconcile()
        self.x11.value.flags &= ~widths.MIN_SIZE
        self.policy.reconcile()
        self.policy.close()
        self.assertFalse(self.x11.value.flags & widths.MIN_SIZE)

    def test_closed_window_is_unmatched_and_restored(self):
        self.policy.reconcile()
        self.policy.event({"WindowClosed": {"id": 1}})
        self.policy.reconcile()
        self.assertEqual(self.x11.value.min_width, 1600)

    def test_title_update_does_not_temporarily_restore_old_constraints(self):
        self.policy.reconcile()
        self.x11.identity = lambda window: ({"fixture"}, "New title")
        self.policy.reconcile()
        self.assertEqual(self.x11.value.min_width, 1)
        self.window["title"] = "New title"
        self.policy.event({"WindowOpenedOrChanged": {"window": self.window}})
        self.policy.reconcile()
        self.assertEqual(self.x11.value.min_width, 1)

    def test_restore_respects_app_new_maximum(self):
        self.policy.reconcile()
        self.x11.value.max_width = 900
        self.policy.close()
        self.assertEqual(self.x11.value.min_width, 900)
        self.assertEqual(self.x11.value.max_width, 900)

    def test_native_terminal_title_updates_do_not_query_x11_again(self):
        self.policy.reconcile()
        changed = self.policy.event(
            {
                "WindowOpenedOrChanged": {
                    "window": {
                        "id": 2,
                        "app_id": "nacre-ai-task",
                        "title": "Spinner update",
                        "is_floating": False,
                    }
                }
            }
        )
        self.assertFalse(changed)
        self.assertIn(2, self.policy.windows)

    def test_only_newly_opened_constrained_window_gets_one_initial_correction(self):
        self.policy.reconcile()
        calls = []
        self.policy.apply_openings(calls.append, float("inf"))
        self.assertEqual(calls, [])
        second = dict(self.window, id=2)
        self.policy.event({"WindowOpenedOrChanged": {"window": second}})
        self.x11.value.min_width = 2400
        self.policy.reconcile()
        self.policy.apply_openings(calls.append, float("inf"))
        self.assertEqual(calls, [2])
        self.x11.value.min_width = 2800
        self.policy.reconcile()
        self.policy.apply_openings(calls.append, float("inf"))
        self.assertEqual(calls, [2])

    def test_pending_opening_correction_is_cancelled_by_floating(self):
        second = dict(self.window, id=2)
        self.policy.event({"WindowOpenedOrChanged": {"window": second}})
        self.policy.reconcile()
        second["is_floating"] = True
        self.policy.event({"WindowOpenedOrChanged": {"window": second}})
        self.policy.reconcile()
        calls = []
        self.policy.apply_openings(calls.append, float("inf"))
        self.assertEqual(calls, [])
