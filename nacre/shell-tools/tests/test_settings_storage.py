"""Settings IDs and atomic patches in private state."""

import concurrent.futures
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]


class SettingsStorageTests(unittest.TestCase):
    def setUp(self):
        self.helper = Path(
            os.environ.get("NACRE_HELPER_BINARY", ROOT / "target/debug/nacre-helper")
        )
        if not self.helper.is_file():
            self.skipTest("Native helper unavailable")

    def environment(self, root):
        home = root / "home"
        home.mkdir()
        env = dict(
            os.environ,
            HOME=str(home),
            XDG_CONFIG_HOME=str(home / ".config"),
            XDG_STATE_HOME=str(home / ".local/state"),
            XDG_CACHE_HOME=str(home / ".cache"),
            XDG_RUNTIME_DIR=str(root / "runtime"),
            QT_QPA_PLATFORM="offscreen",
        )
        for key in [
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "NIRI_SOCKET",
            "HYPRLAND_INSTANCE_SIGNATURE",
            "NACRE_COMPOSITOR_TOOLS",
            "NACRE_PEARL_TOOLS",
            "QML_IMPORT_PATH",
            "QML2_IMPORT_PATH",
            "QT_LOGGING_RULES",
        ]:
            env.pop(key, None)
        (root / "runtime").mkdir(mode=0o700)
        return home, env

    def call(self, env, action, id, payload=None):
        return subprocess.run(
            [str(self.helper), "state-file", action, id],
            input=json.dumps(payload) + "\n" if payload is not None else None,
            env=env,
            text=True,
            capture_output=True,
            timeout=8,
        )

    def test_owned_ids_concurrent_fields_and_future_version(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            home, env = self.environment(root)
            loaded = self.call(env, "load-id", "power")
            self.assertEqual(loaded.returncode, 0, loaded.stdout)
            self.assertFalse(json.loads(loaded.stdout)["exists"])
            self.assertFalse((home / ".config/nacre/power.json").exists())
            with concurrent.futures.ThreadPoolExecutor(max_workers=4) as executor:
                results = list(
                    executor.map(
                        lambda index: self.call(
                            env,
                            "set-id",
                            "power",
                            {"key": ["field" + str(index)], "value": index},
                        ),
                        range(12),
                    )
                )
            for result in results:
                self.assertEqual(result.returncode, 0, result.stdout)
            path = home / ".config/nacre/power.json"
            saved = json.loads(path.read_bytes())
            for index in range(12):
                self.assertEqual(saved["field" + str(index)], index)
            self.assertEqual(saved["ac"], "balanced")
            self.assertEqual(saved["battery"], "power-saver")
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
            for id in ["brain-auth", "../desktop", "scheme", "theme-targets"]:
                refused = self.call(env, "set-id", id, {"key": ["value"], "value": 1})
                self.assertNotEqual(refused.returncode, 0, refused.stdout)
            for key in ["__proto__", "constructor", "version"]:
                self.assertNotEqual(
                    self.call(
                        env, "set-id", "power", {"key": [key], "value": 1}
                    ).returncode,
                    0,
                )
            original = b'{"version":99,"future":"kept"}'
            path.write_bytes(original)
            self.assertNotEqual(
                self.call(
                    env, "set-id", "power", {"key": ["ac"], "value": "balanced"}
                ).returncode,
                0,
            )
            self.assertEqual(path.read_bytes(), original)
