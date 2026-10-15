"""Memory sampling must distinguish unavailable data from zero usage."""

import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "performance", Path(__file__).resolve().parents[1] / "performance.py"
)
performance = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(performance)


class MemoryTests(unittest.TestCase):
    def test_real_exited_child_is_not_retained_as_unknown_live_memory(self):
        child = subprocess.Popen(
            [sys.executable, "-c", "pass"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        try:
            os.waitid(os.P_PID, child.pid, os.WEXITED | os.WNOWAIT)
            self.assertEqual(performance.process(child.pid)["state"], "Z")
            self.assertEqual(performance.snapshot(child.pid, detached=False), {})
        finally:
            child.wait()

    def test_exit_race_does_not_hide_unknown_live_memory_or_measured_swap(self):
        row = {"pid": 42, "ppid": 0, "start": 1, "args": [], "uid": 0, "state": "S"}
        missing = {"pss": None, "swap_pss": None, "memory_error": "ProcessLookupError"}
        with tempfile.TemporaryDirectory() as directory:
            proc = Path(directory)
            (proc / "42").mkdir()
            for state, error, swap, retained in [
                ("Z", "ProcessLookupError", None, False),
                ("X", "FileNotFoundError", None, False),
                ("S", "ProcessLookupError", None, True),
                ("Z", "PermissionError", None, True),
                ("Z", None, 4096, True),
            ]:
                with (
                    self.subTest(state=state, error=error, swap=swap),
                    patch.object(
                        performance,
                        "process",
                        side_effect=[
                            dict(row),
                            dict(row, state=state),
                            dict(row, state=state),
                        ],
                    ),
                    patch.object(
                        performance,
                        "memory",
                        return_value=dict(missing, memory_error=error, swap_pss=swap),
                    ),
                ):
                    result = performance.snapshot(42, proc, detached=False)
                    self.assertEqual(42 in result, retained)
                    if retained:
                        self.assertEqual(result[42]["swap_pss"], swap)

    def test_single_exit_race_retry_preserves_swap_and_checks_identity_again(self):
        row = {"pid": 42, "ppid": 0, "start": 1, "args": [], "uid": 0, "state": "R"}
        missing = {"pss": None, "swap_pss": None, "memory_error": "ProcessLookupError"}
        known = {"pss": 4096, "swap_pss": 4096, "memory_error": None}
        with tempfile.TemporaryDirectory() as directory:
            proc = Path(directory)
            (proc / "42").mkdir()
            for final, expected in [(dict(row), True), (dict(row, start=2), False)]:
                with (
                    self.subTest(final=final),
                    patch.object(
                        performance,
                        "process",
                        side_effect=[dict(row), dict(row), final],
                    ),
                    patch.object(
                        performance, "memory", side_effect=[missing, known]
                    ) as read,
                ):
                    result = performance.snapshot(42, proc, detached=False)
                    self.assertEqual(read.call_count, 2)
                    self.assertEqual(42 in result, expected)
                    if expected:
                        self.assertEqual(result[42]["swap_pss"], 4096)

    def test_missing_swap_field_stays_unknown_when_other_memory_is_available(self):
        with tempfile.TemporaryDirectory() as directory:
            proc = Path(directory)
            (proc / "42").mkdir()
            (proc / "42/smaps_rollup").write_text(
                "Rss: 100 kB\nPss: 60 kB\nPrivate_Clean: 20 kB\nPrivate_Dirty: 30 kB\n"
            )
            row = performance.memory(42, proc)
            self.assertEqual(row["pss"], 60 * 1024)
            self.assertIsNone(row["swap_pss"])

    def test_dead_shell_invalidates_capture_even_when_its_last_memory_read_succeeded(
        self,
    ):
        with patch.object(
            performance, "snapshot", return_value={42: {"state": "Z", "start": 1}}
        ):
            with self.assertRaisesRegex(ValueError, "not running"):
                performance.idle(42, 0, 0.01)

    def test_nearest_rank_p95_does_not_promote_a_single_twentieth_outlier(self):
        result = performance.summarize([*range(1, 20), 1000])
        self.assertEqual(result["p95"], 19)
        self.assertEqual(result["max"], 1000)
        self.assertIsNone(performance.summarize([]))

    def test_rollup_counts_shared_pages_proportionally_and_private_pages_once(self):
        with tempfile.TemporaryDirectory() as directory:
            proc = Path(directory)
            (proc / "42").mkdir()
            (proc / "42/smaps_rollup").write_text(
                "0000-ffff ---p 00000000 00:00 0 [rollup]\n"
                "Rss: 100 kB\nPss: 60 kB\nPrivate_Clean: 20 kB\n"
                "Private_Dirty: 30 kB\nSwapPss: 7 kB\nPss_Anon: 50 kB\n"
            )
            row = performance.memory(42, proc)
            self.assertEqual(row["rss"], 100 * 1024)
            self.assertEqual(row["pss"], 60 * 1024)
            self.assertEqual(row["uss"], 50 * 1024)
            self.assertEqual(row["swap_pss"], 7 * 1024)
            self.assertIsNone(row["memory_error"])

    def test_missing_malformed_and_denied_are_unknown(self):
        with tempfile.TemporaryDirectory() as directory:
            proc = Path(directory)
            (proc / "42").mkdir()
            for content in (None, "Pss: invalid kB\n", "Pss: 4 kB\n"):
                if content is not None:
                    (proc / "42/smaps_rollup").write_text(content)
                row = performance.memory(42, proc)
                self.assertIsNone(row["pss"])
                self.assertIsNone(row["uss"])
                self.assertTrue(row["memory_error"])
        with patch.object(Path, "read_text", side_effect=PermissionError):
            self.assertEqual(performance.memory(42)["memory_error"], "PermissionError")

    def test_incomplete_aggregate_is_not_an_underestimate(self):
        rows = {
            1: {"rss": 1048576, "pss": 524288, "uss": 262144, "swap_pss": 0},
            2: {"rss": 1048576, "pss": None, "uss": None, "swap_pss": None},
        }
        sample = performance.memory_sample(rows, 1)
        self.assertEqual(sample["all_rss_mib"], 2)
        self.assertEqual(sample["shell_pss_mib"], 0.5)
        self.assertIsNone(sample["all_pss_mib"])
        self.assertIsNone(sample["all_uss_mib"])

    def test_snapshot_drops_pid_reused_during_rollup(self):
        row = {"pid": 42, "ppid": 0, "start": 1, "args": [], "uid": 0}
        with tempfile.TemporaryDirectory() as directory:
            proc = Path(directory)
            (proc / "42").mkdir()
            with (
                patch.object(
                    performance, "process", side_effect=[row, dict(row, start=2)]
                ),
                patch.object(performance, "memory", return_value={"pss": 1}),
            ):
                self.assertEqual(performance.snapshot(42, proc, detached=False), {})
