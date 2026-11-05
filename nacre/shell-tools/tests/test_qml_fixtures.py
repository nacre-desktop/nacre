"""Regression tests for isolated QML test process result aggregation."""

import subprocess
import tempfile
from pathlib import Path
import unittest
from unittest.mock import patch

from qml_fixtures import run_qml


class QmlFixtureRunnerTests(unittest.TestCase):
    def test_isolated_runner_preserves_signal_failure_and_reports_every_case(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            (target / "tst_example.qml").write_text(
                'TestCase { name:"Example"; function test_first(){} '
                "function test_second(){} }"
            )
            child_results = [
                subprocess.CompletedProcess([], 0, "first output", ""),
                subprocess.CompletedProcess([], -11, "crashed output", "signal 11"),
            ]
            with (
                patch("qml_fixtures.install_paths"),
                patch("qml_fixtures.install_modules", return_value="fixture-imports"),
                patch("qml_fixtures.subprocess.run", side_effect=child_results),
            ):
                result = run_qml(
                    ["qmltestrunner", "-input", str(target)],
                    isolate=True,
                    capture_output=True,
                    text=True,
                )

        self.assertEqual(result.returncode, -11)
        self.assertIn("Example::test_first: exit=0", result.stdout)
        self.assertIn("Example::test_second: exit=-11", result.stdout)
        self.assertIn("first output", result.stdout)
        self.assertIn("crashed output", result.stdout)
        self.assertIn("signal 11", result.stderr)


if __name__ == "__main__":
    unittest.main()
