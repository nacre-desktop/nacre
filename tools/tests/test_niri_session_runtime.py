"""Niri code overlays never replace the working Hyprland source or preferences."""

from pathlib import Path
import tempfile
import unittest
import os
import subprocess
import shutil
import hashlib
import json
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
import native_niri_session_runtime as runtime


class NiriRuntimeTests(unittest.TestCase):
    def test_width_observer_uses_prepared_code_without_switching_compositor(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            shell = (
                home
                / ".local/share/nacre/niri-on-pc/session/home-view/.local/share/nacre/shell"
            )
            for name in (
                "source/shell.qml",
                "tools/nacre_native_compositor.py",
                "bin/qs",
            ):
                path = shell / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.touch()
            environment = {
                "NACRE_COMPOSITOR": "niri",
                "NIRI_SOCKET": "/test/niri.sock",
                "DISPLAY": ":8",
            }
            with patch.object(
                runtime,
                "overview_hover_binary",
                side_effect=AssertionError(
                    "Observer must not read a large compositor artifact"
                ),
            ):
                args, env = runtime.command("widths", environment, home)
            self.assertEqual(
                args[-2:],
                [
                    str(home / ".local/share/nacre/shell/tools/nacre-helper"),
                    "niri_widths",
                ],
            )
            self.assertEqual(env["NIRI_SOCKET"], "/test/niri.sock")
            self.assertEqual(env["DISPLAY"], ":8")
            self.assertNotIn("--session", args)
            check, _ = runtime.command("widths-check", environment, home)
            self.assertEqual(check[-1], "--check-session")
            with self.assertRaises(ValueError):
                runtime.command("widths", {"NACRE_COMPOSITOR": "hyprland"}, home)

    def test_hover_binary_requires_verified_hash_and_matching_stock_version(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            root = home / ".local/share/nacre/niri-on-pc/overview-hover"
            root.mkdir(parents=True)
            binary = root / "niri"
            binary.write_text("verified experiment")
            binary.chmod(0o700)
            manifest = {
                "schema": 1,
                "enabled": True,
                "nestedVerified": True,
                "sourceRevision": runtime.HOVER_SOURCE,
                "protocol": "NacreOverviewHoverStream",
                "binarySha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            }
            (root / "manifest.json").write_text(json.dumps(manifest))
            with patch.object(
                runtime.subprocess, "check_output", return_value="niri 26.04 (stock)"
            ):
                self.assertEqual(runtime.overview_hover_binary(home), binary)
                binary.write_text("changed after verification")
                self.assertIsNone(runtime.overview_hover_binary(home))
            manifest["binarySha256"] = hashlib.sha256(binary.read_bytes()).hexdigest()
            (root / "manifest.json").write_text(json.dumps(manifest))
            with patch.object(
                runtime.subprocess, "check_output", return_value="niri 26.12 (new)"
            ):
                self.assertIsNone(runtime.overview_hover_binary(home))
            manifest["nestedVerified"] = False
            (root / "manifest.json").write_text(json.dumps(manifest))
            self.assertIsNone(runtime.overview_hover_binary(home))

    def test_palette_prefers_live_publisher_with_offline_seed_fallback(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            config = home / "stage/config"
            self.assertEqual(
                runtime.palette_include(home, config), config / "colors.kdl"
            )
            live = home / ".config/nacre/niri/palette.kdl"
            live.parent.mkdir(parents=True)
            live.write_text("palette")
            self.assertEqual(runtime.palette_include(home, config), live)

    def test_saved_scale_and_scroll_are_seeded_without_hyprland_writes(self):
        text = runtime.host_preferences(
            {"naturalScroll": True, "displays": [{"name": "eDP-1", "scale": 1.5}]}
        )
        self.assertIn("natural-scroll", text)
        self.assertTrue(text.startswith("prefer-no-csd\n"))
        self.assertIn("scale 1.5", text)
        self.assertIn("toggle-overview", text)
        self.assertIn("focus-column-left", text)
        self.assertIn(
            "layout {\n    gaps 12\n    default-column-width { proportion 0.5; }", text
        )
        self.assertIn("Super+M { maximize-column; }", text)
        self.assertIn("gaps 12", text)
        self.assertIn("focus-ring { width 1; }", text)
        self.assertIn('match app-id="^nacre-ai-(dashboard|task)$"', text)
        self.assertIn("default-column-width { proportion 0.5; }", text)
        self.assertIn("Super+R { switch-preset-column-width; }", text)
        self.assertNotIn(".config/hypr", text)
        self.assertNotIn(
            "natural-scroll", runtime.host_preferences({"naturalScroll": False})
        )

    def test_invalid_host_preferences_fail_before_install(self):
        for data in (
            {"naturalScroll": "false"},
            {"displays": [{"name": "eDP-1", "scale": float("nan")}]},
            {"displays": [{"name": "eDP-1", "scale": True}]},
            {"gapsOut": -1},
            {"borderSize": "wide"},
        ):
            with self.assertRaises(ValueError):
                runtime.host_preferences(data)

    @unittest.skipUnless(shutil.which("niri"), "Native niri unavailable")
    def test_mixed_scale_host_config_and_aliases_parse(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "config.kdl"
            path.write_text(
                runtime.host_preferences(
                    {
                        "displays": [
                            {"name": "eDP-1", "scale": 1.5},
                            {"name": 'DP "quoted"', "scale": 1},
                        ]
                    }
                )
            )
            result = subprocess.run(
                ["/usr/bin/niri", "validate", "-c", str(path)],
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_niri_manual_lock_is_explicit_and_other_loginctl_actions_pass_through(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            for name, text in {
                "hyprlock": "echo manual-lock",
                "loginctl": 'printf "original:%s\\n" "$*"',
                "pgrep": "exit 1",
            }.items():
                p = root / name
                p.write_text("#!/bin/sh\n" + text + "\n")
                p.chmod(0o700)
            script = root / "wrapper"
            text = (ROOT / "uwsm/niri-session-bin/loginctl").read_text()
            for name in ("hyprlock", "loginctl"):
                text = text.replace("/usr/bin/" + name, str(root / name))
            script.write_text(text)
            env = dict(os.environ, PATH=str(root) + ":" + os.defpath)
            for backend, arg, expected in (
                ("niri", "lock-session", "manual-lock"),
                ("Hyprland", "lock-session", "original:lock-session"),
                ("niri", "show-session", "original:show-session"),
            ):
                result = subprocess.check_output(
                    ["sh", str(script), arg],
                    env=dict(env, NACRE_COMPOSITOR=backend),
                    text=True,
                )
                self.assertEqual(result.strip(), expected)

    def test_hyprland_executes_original_controller_without_namespace_or_env_changes(
        self,
    ):
        env = {"XDG_CURRENT_DESKTOP": "Hyprland", "NIRI_SOCKET": "stale"}
        argv, child = runtime.command("shell", env, Path("/fixture"))
        self.assertEqual(argv, ["/fixture/.local/bin/nacre-shell", "start"])
        self.assertEqual(child, env)
        argv, child = runtime.command("brain", env, Path("/fixture"))
        self.assertEqual(
            argv,
            [
                "/usr/bin/python3",
                "/fixture/.local/share/nacre/brain/control.py",
                "watch-brain",
            ],
        )
        self.assertEqual(child, env)

    def test_unknown_context_fails_closed(self):
        with self.assertRaisesRegex(ValueError, "explicit niri"):
            runtime.command("shell", {}, Path("/fixture"))

    def test_missing_source_fails_without_falling_back_to_hyprland(self):
        with tempfile.TemporaryDirectory() as folder:
            with self.assertRaisesRegex(ValueError, "separate niri source"):
                runtime.command("shell", {"XDG_CURRENT_DESKTOP": "niri"}, Path(folder))

    def test_niri_overlays_code_only_and_uses_real_session_environment(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            shell = (
                home
                / ".local/share/nacre/niri-on-pc/session/home-view/.local/share/nacre/shell"
            )
            for name in (
                "source/shell.qml",
                "tools/nacre_native_compositor.py",
                "bin/qs",
            ):
                path = shell / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("fixture")
            env = {
                "XDG_CURRENT_DESKTOP": "niri",
                "DBUS_SESSION_BUS_ADDRESS": "real",
                "XDG_RUNTIME_DIR": "/real/runtime",
                "NIRI_SOCKET": "real",
            }
            for action in ("compositor", "shell", "brain"):
                argv, child = runtime.command(action, env, home)
                self.assertIn(str(shell), argv)
                self.assertIn(str(home / ".local/share/nacre/shell"), argv)
                self.assertNotIn("dbus-run-session", argv)
                self.assertNotIn(str(home / ".config/nacre"), argv)
                self.assertEqual(child["DBUS_SESSION_BUS_ADDRESS"], "real")
                self.assertEqual(child["XDG_RUNTIME_DIR"], "/real/runtime")
                self.assertEqual(child["NIRI_SOCKET"], "real")
            argv, _ = runtime.command("brain", env, home)
            self.assertEqual(
                argv[-3:],
                [
                    "/usr/bin/python3",
                    str(home / ".local/share/nacre/brain/control.py"),
                    "watch-brain",
                ],
            )
            argv, _ = runtime.command("compositor", env, home)
            with patch.object(
                runtime, "overview_hover_binary", return_value=Path("/companion/niri")
            ):
                argv, child = runtime.command("compositor", env, home)
                self.assertEqual(argv[-2:], ["/companion/niri", "--session"])
                self.assertEqual(child["NACRE_NIRI_HOVER_STREAM"], "1")
                (home / ".local/share/nacre/niri-on-pc/session/stock-niri").write_text(
                    "explicit opt-out"
                )
                argv, child = runtime.command("compositor", env, home)
                self.assertEqual(child["NACRE_NIRI_HOVER_STREAM"], "0")
            self.assertEqual(argv[-2:], ["/usr/bin/niri", "--session"])
