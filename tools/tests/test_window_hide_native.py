"""Production hide dispatch uses private sockets, XDG roots and visible errors."""

import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "window_hide_socket_peer", ROOT / "tests/parity/helpers/native_compositor_peer.py"
)
peer_module = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(peer_module)


class WindowHideNativeTests(unittest.TestCase):
    def setUp(self):
        self.directory = self.enterContext(tempfile.TemporaryDirectory())
        self.root = Path(self.directory)
        (self.root / "effects.jsonl").touch()
        self.peer = peer_module.Peer(
            self.root,
            [
                {
                    "argv": ["clients", "-j"],
                    "stdout": '[{"address":"0xabc","workspace":{"id":2,"name":"2"}}]',
                },
                {"stdout": "ok\n"},
            ],
        )
        self.addCleanup(self.peer.close)
        self.env = {
            **os.environ,
            "HOME": str(self.root / "home"),
            "XDG_CONFIG_HOME": str(self.root / "home/.config"),
            "XDG_STATE_HOME": str(self.root / "home/.local/state"),
            "XDG_CACHE_HOME": str(self.root / "home/.cache"),
            "XDG_RUNTIME_DIR": str(self.root / "runtime"),
            "DBUS_SESSION_BUS_ADDRESS": "unix:path=" + str(self.root / "absent-bus"),
            "DBUS_SYSTEM_BUS_ADDRESS": "unix:path=" + str(self.root / "absent-system"),
            "HYPRLAND_INSTANCE_SIGNATURE": "fixture",
            "NACRE_COMPOSITOR": "hyprland",
            "PATH": "/nonexistent",
        }
        for key in ("DISPLAY", "WAYLAND_DISPLAY", "NIRI_SOCKET", "XDG_CURRENT_DESKTOP"):
            self.env.pop(key, None)

    def command(self, *args):
        binary = os.environ.get("NACRE_HELPER_BINARY") or str(
            ROOT / "target/release/nacre-helper"
        )
        return subprocess.run(
            [binary, "window-hide", *args],
            env=self.env,
            capture_output=True,
            text=True,
            timeout=5,
            check=False,
        )

    def test_absolute_cache_root_and_relative_fallback_keep_compatible_records(self):
        for value, folder in (
            ("", self.root / "home/.cache"),
            ("relative", self.root / "home/.cache"),
            (str(self.root / "custom-cache"), self.root / "custom-cache"),
        ):
            with self.subTest(xdg=value):
                self.env["XDG_CACHE_HOME"] = value
                result = self.command("--quiet", "hide", "--window", "0xabc")
                self.assertEqual(
                    (result.returncode, result.stdout, result.stderr), (0, "", "")
                )
                record = folder / "hypr-minimize/0xabc"
                self.assertEqual(record.read_text(), "2\n")
                self.assertEqual(record.stat().st_mode & 0o777, 0o600)
        self.assertFalse((self.root / "relative").exists())

    def test_notification_failure_is_visible_without_rolling_back_success(self):
        result = self.command("hide", "--window", "0xabc")
        self.assertEqual((result.returncode, result.stdout), (0, ""))
        self.assertIn("nacre-helper: window feedback failed:", result.stderr)
        self.assertEqual(
            (self.root / "home/.cache/hypr-minimize/0xabc").read_text(), "2\n"
        )

    def test_lock_symlink_is_rejected_before_compositor_access(self):
        folder = self.root / "home/.cache/hypr-minimize"
        folder.mkdir(parents=True)
        target = self.root / "private.txt"
        target.write_text("retain\n")
        target.chmod(0o644)
        (folder / ".lock").symlink_to(target)
        result = self.command("hide", "--window", "0xabc", "--quiet")
        self.assertEqual(result.returncode, 1)
        self.assertTrue(result.stderr)
        self.assertEqual(target.read_text(), "retain\n")
        self.assertEqual(target.stat().st_mode & 0o777, 0o644)
        self.assertEqual((self.root / "effects.jsonl").read_text(), "")
