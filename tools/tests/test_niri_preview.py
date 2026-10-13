"""Windowed preview preserves the daily session and authentication files."""

import importlib.util
import os
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "niri_preview", ROOT / "uwsm/tools/niri-preview.py"
)
preview = importlib.util.module_from_spec(spec)
spec.loader.exec_module(preview)


class NiriPreviewTests(unittest.TestCase):
    def test_command_uses_private_runtime_and_dbus_without_session_import(self):
        env = {
            "HOME": "/fixture",
            "WAYLAND_DISPLAY": "wayland-1",
            "XDG_RUNTIME_DIR": "/run/user/1000",
            "HYPRLAND_INSTANCE_SIGNATURE": "daily",
            "NIRI_SOCKET": "daily",
            "DBUS_SESSION_BUS_ADDRESS": "daily",
            "DISPLAY": ":0",
            "NOTIFY_SOCKET": "daily",
            "PATH": os.defpath,
        }
        original = dict(env)
        command, child = preview.command(
            Path("/private/preview"), Path("/fixture"), env, Path("/private/runtime")
        )
        self.assertEqual(env, original)
        self.assertIn("--ro-bind", command)
        self.assertIn("--unshare-pid", command)
        self.assertIn("/run/dbus", command)
        self.assertIn("dbus-run-session", command)
        self.assertNotIn("--session", command)
        self.assertNotIn("uwsm", command)
        self.assertEqual(child["WAYLAND_DISPLAY"], "/run/user/1000/wayland-1")
        self.assertEqual(child["XDG_RUNTIME_DIR"], "/private/runtime")
        self.assertEqual(child["NACRE_COMPOSITOR"], "niri")
        for key in (
            "HYPRLAND_INSTANCE_SIGNATURE",
            "NIRI_SOCKET",
            "DBUS_SESSION_BUS_ADDRESS",
            "NOTIFY_SOCKET",
            "DISPLAY",
        ):
            self.assertNotIn(key, child)

    def test_missing_parent_display_cannot_start_a_real_session_by_accident(self):
        with self.assertRaisesRegex(ValueError, "running desktop"):
            preview.command(
                Path("/private/preview"), Path("/fixture"), {}, Path("/private/runtime")
            )

    def test_config_allowlist_excludes_credentials_and_account_directories(self):
        for name in ("auth.json", "accounts.json", "ai", ".codex", ".claude"):
            self.assertNotIn(name, preview.SAFE_CONFIG)
        self.assertIn("profile.json", preview.SAFE_CONFIG)
        self.assertNotIn("ai", preview.SAFE_STATE)
        self.assertNotIn("brain", preview.SAFE_STATE)

    def test_private_profile_copy_retains_explicit_permissions_without_changing_source(
        self,
    ):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            source = root / "original-profile.json"
            text = '{"aiFullAccess":true,"workspaceCount":5}'
            source.write_text(text)
            target = root / "private/profile.json"
            preview.private_copy(source, target)
            self.assertEqual(target.read_text(), text)
            self.assertEqual(source.read_text(), text)
            self.assertEqual(target.stat().st_mode & 0o777, 0o600)
