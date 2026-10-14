import importlib.util
from pathlib import Path
import subprocess
import unittest
import os
import tempfile
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
            patch.object(updates, "needs_host_runtime", return_value=False),
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
                "python3": '[[ "$*" == *--needs-host ]] && exit 1; exit 0',
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


class UpdateRuntimeTests(unittest.TestCase):
    def test_namespace_detection(self):
        for mapping, expected in (
            ("0 0 4294967295\n", False),
            ("1000 1000 1\n", True),
            ("0 1000 1\n", True),
        ):
            with patch.object(Path, "read_text", return_value=mapping):
                self.assertEqual(updates.needs_host_runtime(), expected)

    def test_native_package_commands_remain_direct(self):
        with patch.object(updates, "needs_host_runtime", return_value=False):
            self.assertEqual(
                updates.package_command(["checkupdates"]), ["checkupdates"]
            )

    def test_namespaced_check_preserves_output_and_failure(self):
        for code, output, error in (
            (0, "fish 4 -> 5\n", ""),
            (1, "", "Cannot fetch updates"),
        ):
            with (
                patch.object(updates, "needs_host_runtime", return_value=True),
                patch.object(
                    updates.shutil, "which", side_effect=lambda n: n == "checkupdates"
                ),
                patch.object(
                    updates.subprocess,
                    "run",
                    return_value=subprocess.CompletedProcess([], code, output, error),
                ) as run,
            ):
                if code:
                    with self.assertRaises(RuntimeError):
                        updates.preflight()
                else:
                    with patch.object(updates, "qt_update_status", return_value={}):
                        updates.preflight()
                self.assertEqual(
                    run.call_args.args[0],
                    [
                        "systemd-run",
                        "--user",
                        "--wait",
                        "--pipe",
                        "--collect",
                        "--quiet",
                        "--",
                        "checkupdates",
                    ],
                )

    def test_interactive_script_routes_before_confirmation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name, body in {
                "python3": '[[ "$*" == *--needs-host ]]',
                "gum": 'echo "confirmation must happen in the host service"; exit 99',
                "systemd-run": 'printf "%s\\n" "$*"; exit 7',
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
            self.assertIn(
                "--user --wait --pipe --collect --quiet -- /usr/bin/bash", result.stdout
            )
            self.assertNotIn("confirmation must happen", result.stdout)
            self.assertIn("starting the host update service (exit 7)", result.stderr)


if __name__ == "__main__":
    unittest.main()
