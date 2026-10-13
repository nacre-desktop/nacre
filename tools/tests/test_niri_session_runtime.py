"""Niri code overlays never replace the working Hyprland source or preferences."""

import importlib.util
from pathlib import Path
import tempfile
import unittest
import os
import subprocess

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "session_runtime", ROOT / "uwsm/tools/niri-session-runtime.py"
)
runtime = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runtime)


class NiriRuntimeTests(unittest.TestCase):
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
                "tools/compositor_backends/niri.py",
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
            for action in ("compositor", "shell"):
                argv, child = runtime.command(action, env, home)
                self.assertIn(str(shell), argv)
                self.assertIn(str(home / ".local/share/nacre/shell"), argv)
                self.assertNotIn("dbus-run-session", argv)
                self.assertNotIn(str(home / ".config/nacre"), argv)
                self.assertEqual(child["DBUS_SESSION_BUS_ADDRESS"], "real")
                self.assertEqual(child["XDG_RUNTIME_DIR"], "/real/runtime")
                self.assertEqual(child["NIRI_SOCKET"], "real")
            argv, _ = runtime.command("compositor", env, home)
            self.assertEqual(argv[-2:], ["/usr/bin/niri", "--session"])
