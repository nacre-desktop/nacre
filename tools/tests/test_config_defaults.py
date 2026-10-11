"""Truthful filesystem age and current native compositor default contracts."""

from pathlib import Path
import os
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class ConfigDefaultsTests(unittest.TestCase):
    def test_filesystem_age_handles_unknown_future_and_valid_birth(self):
        bash = shutil.which("bash")
        if not bash:
            self.skipTest("Bash unavailable")
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            for birth, now, expected in [
                (0, 200000, "Unknown"),
                (300000, 200000, "Unknown"),
                (27200, 200000, "2 days"),
                ("invalid", 200000, "Unknown"),
            ]:
                for name, value in [("stat", birth), ("date", now)]:
                    path = target / name
                    path.write_text("#!/bin/sh\nprintf '%s\\n' '" + str(value) + "'\n")
                    path.chmod(0o700)
                result = subprocess.run(
                    [bash, str(ROOT / "fastfetch/system-age.sh")],
                    env={**os.environ, "PATH": str(target)},
                    capture_output=True,
                    text=True,
                    check=True,
                )
                self.assertEqual(result.stdout.strip(), expected)
