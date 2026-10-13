import importlib.util
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "updates", Path(__file__).resolve().parents[1] / "updates.py"
)
updates = importlib.util.module_from_spec(spec)
spec.loader.exec_module(updates)


class UpdateTests(unittest.TestCase):
    def check(self, helper_result):
        results = [
            subprocess.CompletedProcess(["checkupdates"], 2, "", ""),
            helper_result,
        ]
        with (
            patch.object(updates.shutil, "which", side_effect=lambda name: name),
            patch.object(updates.subprocess, "run", side_effect=results),
        ):
            return updates.count_updates()

    def test_empty_aur_result_refreshes_to_zero(self):
        result = self.check(subprocess.CompletedProcess(["paru", "-Qua"], 1, "", ""))
        self.assertEqual(result["count"], 0)
        self.assertEqual(result["text"], "0")

    def test_aur_update_is_counted(self):
        result = self.check(
            subprocess.CompletedProcess(["paru", "-Qua"], 0, "chrome 1 -> 2\n", "")
        )
        self.assertEqual(result["count"], 1)

    def test_failed_aur_query_does_not_report_zero(self):
        with self.assertRaises(RuntimeError):
            self.check(
                subprocess.CompletedProcess(
                    ["paru", "-Qua"], 1, "", "error: network failure"
                )
            )


class UpdateFailureTests(unittest.TestCase):
    def test_recovery_build_blocks_pending_qt_but_not_other_packages(self):
        for pending, blocked in (
            ("qt6-base 6.12 -> 6.13\n", True),
            ("qt6-base 6.12.0-2 -> 6.12.0-2.1\n", False),
            ("qt6-wayland 6.12.0-1 -> 6.12.0-1.1\n", False),
            ("qt6-declarative 6.12.0-1.1 -> 6.12.1-1\n", True),
            ("qt6-wayland 6.12.0-1 -> 6.13.0-1\n", True),
            ("fish 4 -> 5\n", False),
        ):
            results = [
                subprocess.CompletedProcess(
                    [], 0, "distributed by Nacre local Qt rebuild", ""
                ),
                subprocess.CompletedProcess([], 0, "", ""),
                subprocess.CompletedProcess([], 0, "qt6-base 6.12.0-2", ""),
            ]
            with patch.object(updates.subprocess, "run", side_effect=results):
                status = updates.qt_update_status(pending)
            self.assertEqual(status["rebuildNeeded"], blocked)

    def test_failure_trap_reports_step_exit_and_keeps_skipreview(self):
        import os
        import tempfile

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name, body in {
                "gum": "exit 0",
                "python3": "exit 0",
                "paru": 'printf "%s\\n" "$*"; exit 7',
            }.items():
                executable = root / name
                executable.write_text("#!/usr/bin/env bash\n" + body + "\n")
                executable.chmod(0o755)
            result = subprocess.run(
                ["bash", str(Path(__file__).parents[1] / "updates.sh")],
                env=dict(os.environ, PATH=str(root) + ":/usr/bin:/bin"),
                stdin=subprocess.DEVNULL,
                timeout=10,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 7)
            self.assertIn("-Syu --skipreview", result.stdout)
            self.assertIn("system package update (exit 7)", result.stderr)


class QtReleaseTests(unittest.TestCase):
    def test_malformed_qt_metadata_is_not_silently_allowed(self):
        with self.assertRaisesRegex(RuntimeError, "Could not parse"):
            updates.pending_qt_release_change("qt6-base unknown update format")

    def test_epoch_change_retains_dependency_guard(self):
        self.assertTrue(
            updates.pending_qt_release_change("qt6-base 6.12.0-2 -> 1:6.12.0-1")
        )


if __name__ == "__main__":
    unittest.main()
