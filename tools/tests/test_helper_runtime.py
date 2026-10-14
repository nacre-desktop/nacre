"""The benchmark must execute the selected baseline/candidate, not infer it."""

import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "helper_runtime", Path(__file__).resolve().parents[1] / "helper_runtime.py"
)
runtime = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runtime)


class HelperRuntimeTests(unittest.TestCase):
    def test_baseline_stays_python_when_the_script_exists(self):
        with tempfile.TemporaryDirectory() as folder:
            tools = Path(folder)
            (tools / "dashboard-data.py").touch()
            (tools / "nacre-helper").touch()
            self.assertEqual(
                runtime.helper_command(tools, "dashboard-data"),
                ([sys.executable, str(tools / "dashboard-data.py")], "Python"),
            )

    def test_native_service_and_fresh_helpers_keep_their_subcommand(self):
        with tempfile.TemporaryDirectory(prefix="helper path ") as folder:
            tools = Path(folder)
            binary = tools / "nacre-helper"
            binary.touch()
            self.assertEqual(
                runtime.helper_command(tools, "helper-service"),
                ([str(binary), "serve"], "Rust"),
            )
            self.assertEqual(
                runtime.helper_command(tools, "dashboard-data"),
                ([str(binary), "dashboard-data"], "Rust"),
            )
            (tools / "dashboard-data.py").touch()
            self.assertEqual(
                runtime.helper_command(tools, "dashboard-data", binary),
                ([str(binary), "dashboard-data"], "Rust"),
            )

    def test_missing_native_binary_is_a_failure_not_a_python_fallback(self):
        with tempfile.TemporaryDirectory() as folder:
            tools = Path(folder)
            (tools / "dashboard-data.py").touch()
            with self.assertRaises(FileNotFoundError):
                runtime.helper_command(tools, "dashboard-data", tools / "absent")

    def test_source_checkout_uses_the_release_candidate_after_python_is_deleted(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            tools = root / "nacre/shell-tools"
            tools.mkdir(parents=True)
            binary = root / "target/release/nacre-helper"
            binary.parent.mkdir(parents=True)
            binary.touch()
            self.assertEqual(
                runtime.helper_command(tools, "dashboard-data"),
                ([str(binary), "dashboard-data"], "Rust"),
            )
