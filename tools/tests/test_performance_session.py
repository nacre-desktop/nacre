"""Performance guards reject unavailable measurements and large regressions."""

import copy
import importlib.util
import json
from pathlib import Path
import sys
import unittest

TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
SPEC = importlib.util.spec_from_file_location(
    "session", TOOLS / "performance-session.py"
)
session = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(session)
BUDGETS = json.loads((TOOLS / "performance-budgets.json").read_text())


def report():
    sample = {
        "pss_mib": {"shell": {"max": 200}},
        "delivered_video_fps": 24,
        "memory_valid_samples": {"pss": {"shell": 2}},
        "samples": [{}, {}],
        "cpu_percent_one_core": {"shell": {"mean": 0.1}},
    }
    timing = {"frame_ms": {"p95": 400}, "synchronous_ms": {"p95": 80}, "samples": [{}]}
    return {
        "scenarios": {name: copy.deepcopy(sample) for name in BUDGETS["ci_pss_mib"]},
        "latency": {
            label: {
                action: copy.deepcopy(timing) for action in ("launcher", "dashboard")
            }
            for label in ("cold", "reopen")
        },
    }


class BudgetTests(unittest.TestCase):
    def test_complete_controlled_capture_passes(self):
        self.assertEqual(session.check_report(report(), BUDGETS), [])

    def test_unknown_memory_is_never_treated_as_zero(self):
        for change in (
            {"pss_mib": {"shell": None}},
            {"memory_valid_samples": {"pss": {"shell": 1}}},
            {"samples": []},
        ):
            with self.subTest(change=change):
                data = report()
                data["scenarios"]["video"].update(change)
                self.assertIn(
                    "incomplete shell PSS coverage",
                    session.check_report(data, BUDGETS)[0],
                )

    def test_memory_peak_and_latency_tail_fail_even_with_low_medians(self):
        data = report()
        data["scenarios"]["all_panels_closed"]["pss_mib"]["shell"].update(
            max=1000, median=200
        )
        data["latency"]["reopen"]["launcher"]["frame_ms"].update(p95=1500, median=400)
        failures = session.check_report(data, BUDGETS)
        self.assertEqual(len(failures), 2)
        self.assertIn("shell PSS", failures[0])
        self.assertIn("frame p95", failures[1])

    def test_stalled_video_cannot_pass_with_low_memory(self):
        data = report()
        data["scenarios"]["video"]["delivered_video_fps"] = 0
        failures = session.check_report(data, BUDGETS)
        self.assertEqual(len(failures), 1)
        self.assertIn("stalled video frame delivery", failures[0])

    def test_missing_keyboard_capture_and_busy_idle_fail(self):
        data = report()
        data["latency"]["cold"].pop("dashboard")
        data["scenarios"]["static_after_latency"]["cpu_percent_one_core"]["shell"][
            "mean"
        ] = 10
        failures = session.check_report(data, BUDGETS)
        self.assertEqual(len(failures), 2)
        self.assertIn("missing frame timing", failures[0])
        self.assertIn("static idle CPU", failures[1])


if __name__ == "__main__":
    unittest.main()
