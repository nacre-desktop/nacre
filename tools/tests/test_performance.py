import importlib.util
from pathlib import Path
import tempfile
import json
import shutil
import subprocess
import sys
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "performance", Path(__file__).parents[1] / "performance.py"
)
measurement = importlib.util.module_from_spec(spec)
spec.loader.exec_module(measurement)


class PerformanceTests(unittest.TestCase):
    def test_production_dashboard_owner_benchmark_completes_real_requests(self):
        if not shutil.which("quickshell"):
            self.skipTest("Quickshell unavailable")
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "owner.json"
            result = subprocess.run(
                [
                    sys.executable,
                    str(Path(__file__).parents[1] / "benchmark-services.py"),
                    "--count",
                    "3",
                    "--output",
                    str(output),
                ],
                stdin=subprocess.DEVNULL,
                capture_output=True,
                text=True,
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            report = json.loads(output.read_text())
            self.assertEqual(report["requests"], 3)
            self.assertEqual(len(report["runs_ms"]), 3)
            self.assertTrue(all(value >= 0 for value in report["runs_ms"]))

    def test_proc_stat_handles_spaces_and_parentheses_and_pid_reuse(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            row = root / "123"
            row.mkdir()
            fields = ["S", "1", *(["0"] * 22)]
            fields[11:15] = ["20", "5", "7", "2"]
            fields[19] = "100"
            fields[21] = "4"
            (row / "stat").write_text("123 (a ) process) " + " ".join(fields))
            (row / "cmdline").write_bytes(b"python3\0helper.py\0")
            sample = measurement.process(123, root)
            self.assertEqual(sample["name"], "a ) process")
            self.assertEqual(sample["ticks"], 25)
            self.assertEqual(sample["child_ticks"], 9)
            self.assertEqual(sample["start"], 100)
            self.assertIsNone(measurement.process(999, root))

    def test_descendants_include_grandchildren_without_other_shells(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for pid in (10, 11, 12, 20):
                (root / str(pid)).mkdir()
            rows = {
                pid: {"pid": pid, "ppid": parent, "uid": 1000, "args": [], "start": 1}
                for pid, parent in ((10, 1), (11, 10), (12, 11), (20, 1))
            }
            with patch.object(
                measurement, "process", side_effect=lambda pid, _: rows[int(pid)]
            ):
                self.assertEqual(
                    set(measurement.snapshot(10, root, False)), {10, 11, 12}
                )

    def test_completion_requires_all_predicates_and_records_distribution(self):
        self.assertFalse(
            measurement.ready({"busy": False}, {"busy": False, "visible": True})
        )
        self.assertTrue(
            measurement.ready(
                {"busy": False, "visible": True}, {"busy": False, "visible": True}
            )
        )
        self.assertEqual(measurement.summarize([1, 2, 3])["median"], 2)
        self.assertEqual(measurement.summarize([1, 2, 3])["p95"], 3)
        self.assertIsNone(measurement.summarize([]))

    def test_action_cleanup_runs_on_failure(self):
        calls = []

        def call(pid, *args):
            calls.append(args)
            return "invalid" if args == ("state", "get") else ""

        with (
            patch.object(measurement, "call", side_effect=call),
            patch.object(measurement.time, "sleep"),
        ):
            with self.assertRaises(ValueError):
                measurement.actions(
                    1,
                    {
                        "test": {
                            "call": ["open", "now"],
                            "state": ["state", "get"],
                            "expected": {},
                            "cleanup": [["close", "now"]],
                        }
                    },
                    1,
                )
        self.assertEqual(calls[-1], ("close", "now"))
