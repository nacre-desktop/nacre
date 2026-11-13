import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
INSTALLERS = (
    "ai/install-component.py",
    "ai/install-codex.py",
    "ai/install-obsidian.py",
    "ai/configure-remote-shell.py",
    "nacre/shell-tools/install-thunar.py",
    "nacre/shell-tools/install-dolphin.py",
    "nacre/shell-tools/install-extras.py",
)


class InstallerArgumentTests(unittest.TestCase):
    def test_help_and_unknown_arguments_exit_before_writing_home(self):
        for installer in INSTALLERS:
            for argument, expected in (("--help", 0), ("--nacre-unknown", 2)):
                with (
                    self.subTest(installer=installer, argument=argument),
                    tempfile.TemporaryDirectory() as home,
                ):
                    environment = {
                        **os.environ,
                        "HOME": home,
                        "XDG_CONFIG_HOME": str(Path(home) / "config"),
                        "XDG_DATA_HOME": str(Path(home) / "data"),
                        "XDG_STATE_HOME": str(Path(home) / "state"),
                    }
                    result = subprocess.run(
                        [sys.executable, str(ROOT / installer), argument],
                        cwd=ROOT,
                        env=environment,
                        capture_output=True,
                        text=True,
                        timeout=10,
                        check=False,
                    )
                    self.assertEqual(result.returncode, expected, result.stderr)
                    if argument == "--help":
                        self.assertIn("usage:", result.stdout.lower())
                    else:
                        self.assertIn("unrecognized arguments", result.stderr)
                    self.assertEqual(list(Path(home).iterdir()), [])


if __name__ == "__main__":
    unittest.main()
