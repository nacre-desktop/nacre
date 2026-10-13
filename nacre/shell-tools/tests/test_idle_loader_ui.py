"""Exercise real Qt Loader destruction, warm reopen and view-state restoration."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class IdleLoaderTests(unittest.TestCase):
    def test_native_lifetime_and_state(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            shutil.copy2(ROOT.parent / "shell/widgets/NacreIdleLoader.qml", target)
            shutil.copy2(ROOT / "tests/idle-loader-qml/tst_idle.qml", target)
            result = subprocess.run(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                capture_output=True,
                text=True,
                timeout=15,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
