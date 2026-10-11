"""Exercise the real greeter against synthetic models and a fake auth proxy."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class LoginUITests(unittest.TestCase):
    def test_native_submission_guards_and_failure_recovery(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            for name in ("Main.qml", "Logo.qml", "LogoData.js"):
                shutil.copy2(ROOT.parent / "login" / name, target / name)
            shutil.copy2(
                ROOT / "tests/login-qml/tst_login.qml", target / "tst_login.qml"
            )
            result = subprocess.run(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env={
                    **os.environ,
                    "QT_QPA_PLATFORM": "offscreen",
                    "QT_QUICK_CONTROLS_STYLE": "Basic",
                },
                capture_output=True,
                text=True,
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
