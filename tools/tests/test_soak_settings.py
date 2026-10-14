"""Opt-in native regression for repeatedly remapping the settings window."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


@unittest.skipUnless(
    os.environ.get("NACRE_RUN_SOAK_REGRESSIONS") == "1",
    "Opt in with NACRE_RUN_SOAK_REGRESSIONS=1 inside a Wayland test session",
)
class SettingsRemapRegression(unittest.TestCase):
    def test_reopened_settings_acknowledges_configure_before_rendering(self):
        for command in ("niri", "quickshell", "bwrap", "dbus-run-session"):
            if not shutil.which(command):
                self.skipTest(f"Native regression requires {command}")
        # The driver validates the parent socket and isolates all guest state.
        with tempfile.TemporaryDirectory(prefix="nacre-settings-remap-") as directory:
            output = Path(directory) / "capture"
            command = [
                sys.executable,
                str(ROOT / "tools/soak.py"),
                "--backend",
                "niri",
                "--hours",
                "0.05",
                "--warmup",
                "10",
                "--interval",
                "10",
                "--action-interval",
                "0.5",
                "--actions",
                "settings",
                "close",
                "--wayland-debug",
                "--output",
                str(output),
            ]
            with (Path(directory) / "driver.log").open("w") as stream:
                result = subprocess.run(
                    command, cwd=ROOT, stdout=stream, stderr=stream, timeout=300
                )
            report = json.loads((output / "niri/report.json").read_text())
            self.assertEqual(result.returncode, 0, report["failure"])
            self.assertTrue(report["workload_healthy"], report["failure"])
            self.assertTrue(report["duration_complete"])
            self.assertGreaterEqual(report["actions"].get("settings", 0), 10)
            self.assertEqual(report["shell_exits"], 0)
            self.assertEqual(report["action_failures"], {})
            self.assertNotIn(
                "initial configure", (output / "niri/shell.log").read_text()
            )


if __name__ == "__main__":
    unittest.main()
