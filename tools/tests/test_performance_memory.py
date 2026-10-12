"""Memory sampling must distinguish unavailable data from zero usage."""

import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "performance", Path(__file__).resolve().parents[1] / "performance.py"
)
performance = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(performance)


class MemoryTests(unittest.TestCase):
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
