"""Exercise the real input producer and rollback without any desktop socket."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]


class SettingsInputTests(unittest.TestCase):
    def test_read_validation_and_failed_apply_preserve_owned_state(self):
        helper = Path(
            os.environ.get("NACRE_HELPER_BINARY", ROOT / "target/debug/nacre-helper")
        )
        if not helper.is_file():
            self.skipTest("Build the native helper")
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            config = home / ".config/nacre/niri"
            config.mkdir(parents=True)
            loader = config / "config.kdl"
            loader.write_text('include "desktop.kdl"\n')
            (config / "desktop.kdl").write_text("")
            env = {
                **os.environ,
                "HOME": str(home),
                "XDG_CONFIG_HOME": str(home / ".config"),
                "XDG_STATE_HOME": str(home / ".local/state"),
                "NACRE_COMPOSITOR": "niri",
                "NIRI_SOCKET": str(home / "absent.sock"),
                "NIRI_CONFIG": str(loader),
            }
            for name in ("NACRE_NIRI_CONFIG", "NACRE_NIRI_DESKTOP_CONFIG"):
                env.pop(name, None)

            def call(*args):
                return subprocess.run(
                    [str(helper), "input-preferences", *args],
                    env=env,
                    capture_output=True,
                    text=True,
                    timeout=15,
                )

            path = home / ".config/nacre/software-input.json"
            result = call("state")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertTrue(json.loads(result.stdout)["ok"])
            self.assertFalse(path.exists())
            for args in [
                ("layouts", 'no"; off'),
                ("touchpad", "not-a-boolean"),
                ("layouts", "no"),
            ]:
                result = call(*args)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(json.loads(result.stdout)["ok"])
                self.assertFalse(
                    path.exists(),
                    "Failed native application must roll back its preference",
                )
            future = '{"version":99,"private-choice":"kept"}\n'
            path.write_text(future)
            result = call("state")
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(path.read_text(), future)
