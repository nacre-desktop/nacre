"""Production gate keeps opt-in state intact and follows the shared XDG rules."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class NativeSessionGateTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(self.enterContext(tempfile.TemporaryDirectory()))
        self.home = self.root / "home"
        self.env = {
            **os.environ,
            "HOME": str(self.home),
            "XDG_CONFIG_HOME": str(self.home / ".config"),
            "XDG_STATE_HOME": str(self.home / ".local/state"),
            "XDG_CACHE_HOME": str(self.home / ".cache"),
            "XDG_CURRENT_DESKTOP": "niri",
            "HYPRLAND_INSTANCE_SIGNATURE": "stale",
            "NACRE_COMPOSITOR": "hyprland",
            "PATH": "/nonexistent",
            "DBUS_SESSION_BUS_ADDRESS": "unix:path="
            + str(self.root / "absent-session"),
            "DBUS_SYSTEM_BUS_ADDRESS": "unix:path=" + str(self.root / "absent-system"),
        }
        for key in ("DISPLAY", "WAYLAND_DISPLAY", "NIRI_SOCKET"):
            self.env.pop(key, None)

    def command(self, action):
        binary = os.environ.get("NACRE_HELPER_BINARY") or str(
            ROOT / "target/release/nacre-helper"
        )
        return subprocess.run(
            [binary, "niri-session-gate", action],
            env=self.env,
            capture_output=True,
            text=True,
            timeout=5,
            check=False,
        )

    def record(self, root, contents):
        file = root / "nacre/niri-on-pc/acceptance.json"
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_text(contents)
        file.chmod(0o600)
        return file

    def test_readiness_and_manual_unlock_stay_separate_without_record_writes(self):
        for unlock in (False, True):
            file = self.record(
                self.home / ".config",
                json.dumps(
                    {"phase": "nacre", "backendReady": True, "unlockVerified": unlock}
                ),
            )
            before = file.read_bytes()
            for action, code in (
                ("shell", 0),
                ("observer", 1),
                ("idle", 0 if unlock else 1),
            ):
                result = self.command(action)
                self.assertEqual(
                    (result.returncode, result.stdout, result.stderr), (code, "", "")
                )
            self.assertEqual(file.read_bytes(), before)
            self.assertEqual(file.stat().st_mode & 0o777, 0o600)

    def test_absolute_config_root_and_relative_fallback_are_intended_path_contract(
        self,
    ):
        old = self.record(
            self.home / ".config", '{"phase":"nacre","backendReady":true}'
        )
        custom = self.root / "config"
        new = self.record(custom, '{"phase":"plain","backendReady":true}')
        self.env["XDG_CONFIG_HOME"] = str(custom)
        self.assertEqual(self.command("shell").returncode, 1)
        for value in ("relative", ""):
            self.env["XDG_CONFIG_HOME"] = value
            self.assertEqual(self.command("shell").returncode, 0)
        self.assertEqual(old.read_text(), '{"phase":"nacre","backendReady":true}')
        self.assertEqual(new.read_text(), '{"phase":"plain","backendReady":true}')

    def test_corrupt_acceptance_denies_and_reports_failure(self):
        file = self.record(self.home / ".config", "bad\n")
        result = self.command("idle")
        self.assertEqual((result.returncode, result.stdout), (1, ""))
        self.assertIn("session acceptance unavailable:", result.stderr)
        self.assertEqual(file.read_text(), "bad\n")
        self.env["XDG_CURRENT_DESKTOP"] = "Hyprland"
        result = self.command("idle")
        self.assertEqual((result.returncode, result.stdout, result.stderr), (0, "", ""))
