"""Windowed preview preserves the daily session and authentication files."""

import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
import shutil
import subprocess

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
        self.assertEqual(
            child["NACRE_QS_CONFIG"], "/fixture/.config/quickshell/nacre/shell.qml"
        )
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

    def test_wallpaper_snapshot_dereferences_links_into_hidden_cache(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            asset = root / "cache/poster.png"
            asset.parent.mkdir()
            asset.write_bytes(b"image")
            source = root / "wallpaper"
            source.mkdir()
            (source / "current").symlink_to(asset)
            preview.private_copy(source, root / "snapshot")
            asset.unlink()
            self.assertEqual((root / "snapshot/current").read_bytes(), b"image")
            self.assertFalse((root / "snapshot/current").is_symlink())

    def test_presentation_copies_referenced_cache_assets_only(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            source = home / ".cache/nacre"
            source.mkdir(parents=True)
            (source / "poster.png").write_bytes(b"image")
            (source / "unrelated.json").write_text("private unrelated cache")
            outside = home / "private.txt"
            outside.write_text("outside")
            (source / "escape.png").symlink_to(outside)
            state = home / "snapshot-state"
            state.mkdir()
            (state / "presentation.json").write_text(
                json.dumps(
                    {
                        "poster": str(source / "poster.png"),
                        "escape": str(source / "escape.png"),
                        "outside": str(outside),
                    }
                )
            )
            target = home / "snapshot-cache"
            preview.copy_cached_presentation(home, state, target)
            self.assertEqual((target / "poster.png").read_bytes(), b"image")
            self.assertFalse((target / "unrelated.json").exists())
            self.assertFalse((target / "escape.png").exists())

    def test_managed_runtime_symlink_is_mounted_after_normal_parent(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            original = home / ".local/share/nacre/palette-runtime"
            original.parent.mkdir(parents=True)
            release = home / "managed-release"
            release.mkdir()
            original.symlink_to(release, target_is_directory=True)
            command, _ = preview.command(
                home / "preview",
                home,
                {"WAYLAND_DISPLAY": "wayland-1", "XDG_RUNTIME_DIR": "/runtime"},
                home / "runtime",
            )
            binds = [
                command[i + 1 : i + 3]
                for i, arg in enumerate(command)
                if arg == "--ro-bind"
            ]
            parent = [
                str(home / "preview/home-view/.local/share/nacre"),
                str(original.parent),
            ]
            runtime = [str(release), str(original)]
            self.assertLess(binds.index(parent), binds.index(runtime))

    @unittest.skipUnless(shutil.which("niri"), "Native niri unavailable")
    def test_preview_aliases_parse_with_named_workspace_and_quoted_home(self):
        with tempfile.TemporaryDirectory() as folder:
            config = Path(folder) / "config.kdl"
            config.write_text(
                'workspace "personal workspace"\n'
                + preview.preview_bindings(
                    Path('/fixture/quoted"home'), {1: "personal workspace"}
                )
            )
            result = subprocess.run(
                ["niri", "validate", "-c", str(config)], capture_output=True, text=True
            )
            self.assertEqual(result.returncode, 0, result.stderr)
